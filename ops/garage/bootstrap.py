#!/usr/bin/env python3
"""Provisions the bundled Garage node for PwnForge.

Replaces ops/minio/bootstrap.sh. MinIO Community Edition is end of life; see
docs/adr/0002-replacing-minio.md.

Run by the `garage-init` one-shot container (profile `garage`). Idempotent:
every step either creates something or confirms it is already there, so
re-running it is the supported way to reconcile drift.

WHY PYTHON AND NOT A SHELL SCRIPT
---------------------------------
The `dxflrs/garage` image is built `FROM scratch`: it contains the `garage`
binary and nothing else - no shell, no coreutils. A shell script cannot run in
it, and the `garage` CLI cannot run anywhere else, because remote admin needs
`--rpc-host <full-node-id>@host:port` and the node id is only knowable after
the node has started.

So this talks to Garage's admin HTTP API instead, from an official
`python:*-alpine` image using nothing but the standard library. No image to
build, no packages installed at container start, and the JSON handling is
honest rather than `sed` against curl output.

CREDENTIALS
-----------
The admin token and the RPC secret live in `ops/garage/garage.toml`, which this
container mounts read-only and the backend never sees - the same separation the
MinIO root credential had. The backend only ever holds the S3 key pair created
below, which cannot create or delete a bucket.

ROTATING THE APPLICATION CREDENTIAL
-----------------------------------
This does NOT work the way it did with MinIO, and the difference is not
cosmetic. Garage refuses to change an existing key's secret, and refuses to
reuse an access key id ever again - including after the key is deleted
("Even if it is deleted, we can't let you create a new key with the same ID").

So rotating means issuing a *new* key:

  1. Put a NEW `PWNFORGE_S3_ACCESS_KEY_ID` **and** a new
     `PWNFORGE_S3_SECRET_ACCESS_KEY` in `.env`. Both must change; reusing the
     old id will fail.
  2. `docker compose run --rm garage-init`   (imports and grants the new key)
  3. `docker compose up -d --force-recreate backend`
  4. Once the backend is healthy, retire the old key:
     `docker compose exec garage /garage key delete --yes <old-access-key-id>`

Step 4 is deliberately not automated: deleting the key the running backend is
still using would take the instance down, and this script has no way to know
whether step 3 has landed.
"""

import json
import os
import re
import sys
import time
import urllib.error
import urllib.request

CONFIG_PATH = os.environ.get("GARAGE_CONFIG_FILE", "/etc/garage.toml")
ADMIN_URL = os.environ.get("GARAGE_ADMIN_URL", "http://garage:3903")

# Relative weight of this node in the partition ring, NOT a storage quota.
#
# Worth being precise about, because the name invites the opposite reading: a
# node advertising 1G happily stores far more than 1G. Measured - a node
# advertising 100M accepted 300 MB across six objects and served every one of
# them back. On a single node the value is simply 100% of the ring and has no
# practical effect; it only starts to matter when a second node is added and
# the two shares have to be weighed against each other.
#
# What actually bounds storage is the filesystem, and, below that, a per-bucket
# quota:
#
#   docker compose exec garage /garage bucket set-quotas <bucket> --max-size 50G
#
# The backend reports a quota refusal as HTTP 507 `storage_quota_exceeded`
# rather than as a generic outage, so setting one is safe.
DEFAULT_CAPACITY_BYTES = 100 * 1024**3  # 100 GiB

ZONE = os.environ.get("PWNFORGE_GARAGE_ZONE", "pwnforge")


def fail(message):
    print(f"bootstrap: {message}", file=sys.stderr)
    sys.exit(1)


def required_env(name):
    value = os.environ.get(name, "").strip()
    if not value:
        fail(f"{name} must be set")
    return value


def capacity_bytes():
    """Parses `PWNFORGE_GARAGE_CAPACITY`, accepting the suffixes an operator
    would reach for (`100G`, `2T`, `500M`) as well as a bare byte count."""
    raw = os.environ.get("PWNFORGE_GARAGE_CAPACITY", "").strip()
    if not raw:
        return DEFAULT_CAPACITY_BYTES

    match = re.fullmatch(r"(\d+)\s*([KMGTP]?)B?", raw, re.IGNORECASE)
    if not match:
        fail(
            f"PWNFORGE_GARAGE_CAPACITY is not a size: {raw!r} "
            "(expected something like 100G, 2T or a byte count)"
        )

    scale = {"": 1, "K": 1024, "M": 1024**2, "G": 1024**3, "T": 1024**4, "P": 1024**5}
    return int(match.group(1)) * scale[match.group(2).upper()]


def admin_token():
    """Reads `admin_token` out of garage.toml.

    Kept in exactly one place rather than duplicated into the environment: the
    token is an administrative credential, and the whole point of the split is
    that it lives in the mounted config and nowhere else.
    """
    try:
        with open(CONFIG_PATH, encoding="utf-8") as handle:
            config = handle.read()
    except OSError as error:
        fail(f"could not read {CONFIG_PATH}: {error}")

    match = re.search(r'^\s*admin_token\s*=\s*"([^"]+)"', config, re.MULTILINE)
    if not match:
        fail(
            f"{CONFIG_PATH} has no admin_token under [admin]; "
            "copy ops/garage/garage.toml.example and fill it in"
        )
    return match.group(1)


TOKEN = None


def call(endpoint, payload=None, query="", method=None):
    """One admin API call. Returns (status, decoded body)."""
    url = f"{ADMIN_URL}/v2/{endpoint}{query}"
    data = json.dumps(payload).encode() if payload is not None else None

    request = urllib.request.Request(
        url,
        data=data,
        method=method or ("POST" if data is not None else "GET"),
        headers={
            "Authorization": f"Bearer {TOKEN}",
            "Content-Type": "application/json",
        },
    )

    try:
        with urllib.request.urlopen(request, timeout=15) as response:
            body = response.read().decode()
            return response.status, json.loads(body) if body else None
    except urllib.error.HTTPError as error:
        body = error.read().decode()
        try:
            return error.code, json.loads(body) if body else None
        except json.JSONDecodeError:
            return error.code, {"message": body}


def wait_for_node():
    """The compose healthcheck already gates this container, but a node that
    answers its health probe can still be finishing its admin listener."""
    for attempt in range(1, 16):
        try:
            status, body = call("GetClusterStatus")
            if status == 200:
                return body
            print(f"bootstrap: admin API answered {status} ({attempt}/15)")
        except urllib.error.URLError as error:
            print(f"bootstrap: waiting for Garage at {ADMIN_URL} ({attempt}/15): {error.reason}")
        time.sleep(2)

    fail(f"Garage did not become reachable at {ADMIN_URL}")


def ensure_layout(cluster):
    """Assigns this node a role, once.

    This is the step that does not transpose from the MinIO script. A layout
    carries a version, and `ApplyClusterLayout` only accepts exactly one past
    the current version - so a hardcoded `version: 1`, which works on a fresh
    instance, fails on every later run with "Invalid new layout version". The
    version is therefore read from the cluster rather than assumed, and the
    whole step is skipped once the node has a role.
    """
    nodes = cluster.get("nodes") or []
    if not nodes:
        fail("the cluster reports no nodes")

    node = nodes[0]
    node_id = node["id"]

    if node.get("role") is not None:
        # Deliberately not re-assigned. Changing a node's capacity rewrites the
        # partition ring and moves data between nodes; that is an operator
        # decision, not something a container restart should do unattended. On
        # a single node it would also achieve nothing, since one node is always
        # 100% of the ring.
        print(f"bootstrap: node {node_id[:16]} already has a role, layout left untouched")
        return

    capacity = capacity_bytes()
    print(f"bootstrap: assigning layout (zone {ZONE}, capacity {capacity} bytes)")

    status, body = call(
        "UpdateClusterLayout",
        {"roles": [{"id": node_id, "zone": ZONE, "capacity": capacity, "tags": []}]},
    )
    if status != 200:
        fail(f"UpdateClusterLayout failed with {status}: {body}")

    current = cluster.get("layoutVersion", 0)
    status, body = call("ApplyClusterLayout", {"version": current + 1})
    if status != 200:
        fail(f"ApplyClusterLayout failed with {status}: {body}")

    print(f"bootstrap: layout applied at version {current + 1}")


def ensure_bucket(alias):
    """Creates the bucket, tolerating the 409 a second run produces."""
    status, body = call("CreateBucket", {"globalAlias": alias})

    if status == 200:
        print(f"bootstrap: bucket {alias} created")
        return body["id"]
    if status == 409:
        print(f"bootstrap: bucket {alias} already exists")
    else:
        fail(f"CreateBucket({alias}) failed with {status}: {body}")

    status, body = call("GetBucketInfo", query=f"?globalAlias={alias}")
    if status != 200:
        fail(f"GetBucketInfo({alias}) failed with {status}: {body}")
    return body["id"]


def audit_keys(alias, expected_key_id):
    """Warns when more than one key can reach a bucket.

    This exists because of how rotation works here. Garage will not change a
    key's secret, so rotating means importing a *new* key and deleting the old
    one by hand, and this script cannot do that last step: it has no way to
    know whether the backend has picked the new key up, and deleting the one
    still in use would take the instance down.

    Which means a rotation driven by a compromise leaves the compromised
    credential live until someone remembers step 4. A line of documentation is
    not enough for that, so every run says out loud what can still reach the
    buckets. The operator sees it on the run that created the new key, which is
    the moment they are already looking.

    A warning and not a failure: two keys is a legitimate mid-rotation state,
    and refusing to boot would make the safe path the one that breaks.
    """
    status, body = call("GetBucketInfo", query=f"?globalAlias={alias}")
    if status != 200:
        print(f"bootstrap: could not audit keys on {alias} ({status})")
        return

    def can_reach(key):
        permissions = key.get("permissions") or {}
        return any(permissions.get(name, False) for name in ("read", "write", "owner"))

    others = [
        key.get("accessKeyId")
        for key in body.get("keys") or []
        if key.get("accessKeyId") != expected_key_id and can_reach(key)
    ]

    if not others:
        return

    print(f"bootstrap: WARNING - {alias} is reachable by {len(others) + 1} keys:")
    print(f"bootstrap:   {expected_key_id}  (the one this instance is configured with)")
    for key_id in others:
        print(f"bootstrap:   {key_id}  <-- not configured here")
    print("bootstrap:   If you are rotating, retire the old key once the backend is")
    print("bootstrap:   healthy on the new one. Until then it still works:")
    for key_id in others:
        print(f"bootstrap:     docker compose exec garage /garage key delete --yes {key_id}")
    print("bootstrap:   A rotation prompted by a compromise is not finished until it is gone.")


def ensure_key(access_key_id, secret_access_key):
    """Imports the application key, once.

    A second run does NOT rotate the secret: Garage answers 409 and refuses,
    permanently, to let that access key id be recreated. See the rotation
    procedure in this file's docstring - it requires a new id, not a new
    secret under the old one.
    """
    status, body = call("GetKeyInfo", query=f"?id={access_key_id}")

    if status == 200:
        print(f"bootstrap: key {access_key_id} already exists, secret left as it is")
        if body.get("permissions", {}).get("createBucket"):
            # The grant the MinIO policy had to withhold explicitly. Off by
            # default here, but asserted so an operator who flipped it by hand
            # sees it put back.
            call("UpdateKey", {"allow": {"createBucket": False}}, query=f"?id={access_key_id}")
            print(f"bootstrap: revoked createBucket from {access_key_id}")
        return

    status, body = call(
        "ImportKey",
        {
            "accessKeyId": access_key_id,
            "secretAccessKey": secret_access_key,
            "name": "pwnforge-app",
        },
    )
    if status != 200:
        fail(f"ImportKey({access_key_id}) failed with {status}: {body}")

    print(f"bootstrap: key {access_key_id} imported")


def ensure_grant(bucket_id, alias, access_key_id):
    """Read and write on exactly this bucket.

    No `owner`: ownership would let the key change the bucket's own
    permissions, which is the escalation the MinIO policy avoided by
    withholding the admin actions. Re-granting is a no-op, so this runs every
    time rather than being conditional.
    """
    status, body = call(
        "AllowBucketKey",
        {
            "bucketId": bucket_id,
            "accessKeyId": access_key_id,
            "permissions": {"read": True, "write": True, "owner": False},
        },
    )
    if status != 200:
        fail(f"AllowBucketKey({alias}) failed with {status}: {body}")

    print(f"bootstrap: {access_key_id} can read and write {alias}")


def main():
    global TOKEN

    access_key_id = required_env("PWNFORGE_S3_ACCESS_KEY_ID")
    secret_access_key = required_env("PWNFORGE_S3_SECRET_ACCESS_KEY")
    attachments = required_env("PWNFORGE_S3_BUCKET_ATTACHMENTS")
    avatars = required_env("PWNFORGE_S3_BUCKET_AVATARS")

    if attachments == avatars:
        fail("the attachments and avatars buckets must be different")

    TOKEN = admin_token()

    cluster = wait_for_node()
    print(f"bootstrap: connected to {ADMIN_URL}")

    ensure_layout(cluster)

    ensure_key(access_key_id, secret_access_key)

    for alias in (attachments, avatars):
        bucket_id = ensure_bucket(alias)
        ensure_grant(bucket_id, alias, access_key_id)

    # After the grants, so a freshly imported key is counted and the operator
    # sees the full picture on the run that created it.
    for alias in (attachments, avatars):
        audit_keys(alias, access_key_id)

    print("bootstrap: done")


if __name__ == "__main__":
    main()
