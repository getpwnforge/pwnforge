import { CircleHelpIcon, LayoutGridIcon, LinkIcon, LockIcon, LogInIcon, UsersIcon } from "lucide-react";
import { useEffect, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Link, useNavigate } from "react-router";
import { toast } from "sonner";

import { FormError } from "@/components/common/FormError";
import { RoleBadge } from "@/components/members/RoleBadge";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { RelativeTime } from "@/components/ui/relative-time";
import { Skeleton } from "@/components/ui/skeleton";
import { useAuth } from "@/hooks/useAuth";
import { useJoinTeam, useJoinWorkspace, useTeamInvitationPreview, useWorkspaceInvitationPreview } from "@/hooks/useInvitations";
import { forgetLinkToken, useLinkToken } from "@/hooks/useLinkToken";
import { usePageTitle } from "@/hooks/usePageTitle";
import { apiErrorKey, isApiError } from "@/lib/api-error";
import type { TeamRole } from "@/lib/api/types";
import { ROUTES } from "@/lib/routes";

type Kind = "team" | "workspace";

const CARD = "w-full max-w-md gap-0 p-7 text-center";

/** Where a link's token waits across a sign-in, one per kind. */
const stashOf = (kind: Kind) => `invitation:${kind}`;
/** The page to come back to after signing in: the path only, never the token. */
const linkPathOf = (kind: Kind) => (kind === "team" ? ROUTES.teamInvitation : ROUTES.workspaceInvitation);

/**
 * Where the links of invitation mails and shared team links land. Outside RequireAuth: the token
 * lives in the fragment, which a redirect through /login would drop. useLinkToken reads it, takes
 * it out of the address bar and keeps it in this tab's sessionStorage, so signing in from here
 * comes back with it; it is forgotten once the invitation is joined, refused or put off.
 */
export function InvitationLinkPage({ kind }: Readonly<{ kind: Kind }>) {
  const { t } = useTranslation(["common"]);
  const token = useLinkToken(stashOf(kind));
  const { status, user } = useAuth();
  usePageTitle(t("common:invitationLink.tabTitle"));

  if (!token) return <MissingCard />;
  if (status === "anonymous") return <SignInCard kind={kind} />;
  if (!user) return <LoadingCard />;
  return kind === "team" ? <TeamInvitation token={token} /> : <WorkspaceInvitation token={token} />;
}

const isAlreadyMember = (error: unknown) => isApiError(error) && error.code === "already_member";

function TeamInvitation({ token }: Readonly<{ token: string }>) {
  const { t } = useTranslation(["common", "errors"]);
  const navigate = useNavigate();
  const preview = useTeamInvitationPreview(token);
  const join = useJoinTeam();

  if (preview.isPending) return <LoadingCard />;
  if (preview.isError) return <PreviewError kind="team" error={preview.error} />;

  const name = preview.data.team_name;
  const onJoin = () =>
    join.mutate(token, {
      onSuccess: (team) => {
        forgetLinkToken(stashOf("team"));
        void navigate(ROUTES.team(team.slug), { replace: true });
      },
      onError: (error) => {
        if (!isAlreadyMember(error)) {
          toast.error(t(apiErrorKey(error)));
          return;
        }
        forgetLinkToken(stashOf("team"));
        // The preview names the team but not its slug: its list is where it can be found.
        toast.info(t("common:invitationLink.alreadyMember", { name }));
        void navigate(ROUTES.teams, { replace: true });
      },
    });

  return (
    <InvitationCard
      kind="team"
      name={name}
      role={preview.data.role}
      author={preview.data.invited_by_username}
      expiresAt={preview.data.expires_at}
      pending={join.isPending}
      onJoin={onJoin}
    />
  );
}

function WorkspaceInvitation({ token }: Readonly<{ token: string }>) {
  const { t } = useTranslation(["common", "errors"]);
  const navigate = useNavigate();
  const preview = useWorkspaceInvitationPreview(token);
  const join = useJoinWorkspace();

  if (preview.isPending) return <LoadingCard />;
  if (preview.isError) return <PreviewError kind="workspace" error={preview.error} />;

  const name = preview.data.workspace_name;
  const onJoin = () =>
    join.mutate(token, {
      onSuccess: (workspace) => {
        forgetLinkToken(stashOf("workspace"));
        void navigate(ROUTES.workspace(workspace.slug), { replace: true });
      },
      onError: (error) => {
        if (!isAlreadyMember(error)) {
          toast.error(t(apiErrorKey(error)));
          return;
        }
        forgetLinkToken(stashOf("workspace"));
        toast.info(t("common:invitationLink.alreadyMember", { name }));
        void navigate(ROUTES.workspaces, { replace: true });
      },
    });

  return (
    <InvitationCard
      kind="workspace"
      name={name}
      role={preview.data.role}
      author={preview.data.invited_by_username}
      expiresAt={preview.data.expires_at}
      pending={join.isPending}
      onJoin={onJoin}
    />
  );
}

function InvitationCard({ kind, name, role, author, expiresAt, pending, onJoin }: Readonly<{
  kind: Kind;
  name: string;
  role: TeamRole;
  author: string | null | undefined;
  expiresAt: string;
  pending: boolean;
  onJoin: () => void;
}>) {
  const { t } = useTranslation(["common"]);
  const Icon = kind === "team" ? UsersIcon : LayoutGridIcon;

  return (
    <Card className={CARD}>
      <p className="mb-3.5 text-nav font-medium tracking-wider text-text-subtle uppercase">
        {t(`common:invitationLink.${kind}.eyebrow`)}
      </p>
      <StateIcon accent>
        <Icon aria-hidden className="size-6" />
      </StateIcon>
      <h1 className="text-lg font-semibold tracking-tight text-balance text-text">
        {t(`common:invitationLink.${kind}.title`, { name })}
      </h1>
      <p className="mx-auto mt-1.5 max-w-[34ch] text-sm text-text-muted">
        {author
          ? t(`common:invitationLink.${kind}.lead`, { author: `@${author}` })
          : t(`common:invitationLink.${kind}.leadNoAuthor`)}
      </p>
      <div className="my-5 flex flex-wrap items-center justify-center gap-1.5">
        <RoleBadge role={role} />
        <Badge variant="default">
          {t("common:invitationLink.expires")} <RelativeTime timestamp={expiresAt} />
        </Badge>
      </div>
      <div className="flex flex-col gap-2">
        <Button variant="primary" disabled={pending} onClick={onJoin}>
          <LogInIcon aria-hidden />
          {t(`common:invitationLink.${kind}.join`)}
        </Button>
        <Button asChild variant="ghost">
          <Link to={ROUTES.workspaces} onClick={() => forgetLinkToken(stashOf(kind))}>
            {t("common:invitationLink.later")}
          </Link>
        </Button>
      </div>
      <SignedInAs />
    </Card>
  );
}

/** Who is about to accept: a mailed invitation opened in the wrong account answers 404. */
function SignedInAs() {
  const { t } = useTranslation(["common"]);
  const { user, logout } = useAuth();
  if (!user) return null;
  return (
    <p className="mt-5 flex flex-wrap items-center justify-center gap-1.5 border-t border-border pt-4 text-xs text-text-muted">
      {t("common:invitationLink.signedInAs")} <span className="font-mono">@{user.username}</span>
      <button type="button" className="text-primary hover:underline" onClick={() => void logout()}>
        {t("common:invitationLink.switchAccount")}
      </button>
    </p>
  );
}

function PreviewError({ kind, error }: Readonly<{ kind: Kind; error: unknown }>) {
  const { t } = useTranslation(["common"]);
  // A 404 means the token will never serve again: drop it. A 5xx keeps it, the server may come back.
  const refused = isApiError(error) && error.status === 404;
  useEffect(() => {
    if (refused) forgetLinkToken(stashOf(kind));
  }, [refused, kind]);

  if (!refused) {
    return (
      <Card className={CARD}>
        <FormError error={error} />
      </Card>
    );
  }
  // Unknown, expired, used up or meant for another account: the API does not tell them apart.
  return (
    <Card className={CARD}>
      <StateIcon danger>
        <LinkIcon aria-hidden className="size-5" />
      </StateIcon>
      <h1 className="text-lg font-semibold tracking-tight text-text">{t("common:invitationLink.invalid.title")}</h1>
      <p className="mx-auto mt-1.5 max-w-[34ch] text-sm text-text-muted">{t("common:invitationLink.invalid.text")}</p>
      <div className="my-5 rounded-lg border border-border bg-surface px-3.5 py-3 text-left text-[13px] text-text-muted">
        {t("common:invitationLink.invalid.reasons")}
        <ul className="mt-1.5 list-disc space-y-0.5 pl-5">
          <li>{t("common:invitationLink.invalid.expired")}</li>
          <li>{t("common:invitationLink.invalid.usedUp")}</li>
          <li>{t("common:invitationLink.invalid.otherAccount")}</li>
        </ul>
      </div>
      <div className="flex flex-col gap-2">
        <Button asChild variant="secondary">
          <Link to={kind === "team" ? ROUTES.teams : ROUTES.workspaces}>{t("common:invitationLink.invalid.pending")}</Link>
        </Button>
        <Button asChild variant="ghost">
          <Link to={ROUTES.workspaces}>{t("common:invitationLink.home")}</Link>
        </Button>
      </div>
      <SignedInAs />
    </Card>
  );
}

function SignInCard({ kind }: Readonly<{ kind: Kind }>) {
  const { t } = useTranslation(["common"]);
  return (
    <Card className={CARD}>
      <StateIcon>
        <LockIcon aria-hidden className="size-5" />
      </StateIcon>
      <h1 className="text-lg font-semibold tracking-tight text-text">{t("common:invitationLink.anonymous.title")}</h1>
      <p className="mx-auto mt-1.5 max-w-[34ch] text-sm text-text-muted">{t(`common:invitationLink.anonymous.${kind}`)}</p>
      <div className="mt-5 flex flex-col gap-2">
        <Button asChild variant="primary">
          <Link to={{ pathname: ROUTES.login, search: `?from=${encodeURIComponent(linkPathOf(kind))}` }}>
            {t("common:invitationLink.anonymous.signIn")}
          </Link>
        </Button>
        <Button asChild variant="secondary">
          <Link to={ROUTES.register}>{t("common:invitationLink.anonymous.register")}</Link>
        </Button>
      </div>
      <p className="mt-4 text-xs text-text-muted">{t("common:invitationLink.anonymous.hint")}</p>
    </Card>
  );
}

function MissingCard() {
  const { t } = useTranslation(["common"]);
  return (
    <Card className={CARD}>
      <StateIcon>
        <CircleHelpIcon aria-hidden className="size-5" />
      </StateIcon>
      <h1 className="text-lg font-semibold tracking-tight text-text">{t("common:invitationLink.missing.title")}</h1>
      <p className="mx-auto mt-1.5 max-w-[34ch] text-sm text-text-muted">{t("common:invitationLink.missing.text")}</p>
      <Button asChild variant="secondary" className="mt-5">
        <Link to={ROUTES.workspaces}>{t("common:invitationLink.home")}</Link>
      </Button>
    </Card>
  );
}

function LoadingCard() {
  return (
    <Card className={CARD} aria-busy="true">
      <Skeleton className="mx-auto mb-4 size-12 rounded-xl" />
      <Skeleton className="mx-auto h-5 w-2/3" />
      <Skeleton className="mx-auto mt-2 h-4 w-1/2" />
      <Skeleton className="mt-6 h-9 w-full" />
    </Card>
  );
}

function StateIcon({ accent, danger, children }: Readonly<{ accent?: boolean; danger?: boolean; children: ReactNode }>) {
  let tone = "border border-border bg-surface text-text-muted";
  if (accent) tone = "bg-ember-soft text-primary";
  if (danger) tone = "bg-danger/15 text-danger-text";
  return <div className={`mx-auto mb-4 grid size-12 place-items-center rounded-xl ${tone}`}>{children}</div>;
}
