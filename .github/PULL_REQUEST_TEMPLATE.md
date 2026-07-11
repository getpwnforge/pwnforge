## Description

<!-- What does this PR do, and why? -->

## Related issue

<!-- Closes #123 -->

## Type of change

- [ ] Bug fix
- [ ] New feature
- [ ] Breaking change
- [ ] Documentation
- [ ] Refactor / chore

## Checklist

Before submitting this PR, please ensure the following:

- [ ] **DCO Signed-off**: All commits are signed off (`git commit -s`). CI will only pass if the DCO check passes.
- [ ] **Commit Messages**: Follow the [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) format (`type(scope): summary`)
- [ ] **Coding Style**: Run `cargo fmt` and `cargo clippy --all-targets --all-features -- -D warnings` (backend), or the project lint config (frontend), to ensure your code meets the style guidelines.
- [ ] **Local Tests Passed**: Run `cargo test` / `pnpm test` locally and confirm they pass before opening the PR.
- [ ] **Lockfiles**: `Cargo.lock` / `pnpm-lock.yaml` are committed if dependencies changed.
- [ ] **Tests added**: New auth/permissions logic includes tests for the relevant role transitions.
- [ ] **Docs updated**: If user-facing behavior changed, the relevant docs are updated too.

If you are using AI-assisted tools, please ensure the following:

- [ ] **AI Disclosure**: Disclose it using the `Assisted-by:` trailer in your commits and note it in the PR description. See [AI_POLICY.md](../AI_POLICY.md) for the detailed format.
- [ ] **Human-in-the-loop**: You must fully understand every line you submit and be able to explain it in review. Submitting commits, issues, or PRs without a human actually verifying the content is forbidden. See [AI_POLICY.md](../AI_POLICY.md) for more details.

## Screenshots (if UI change)

<!-- Drag and drop images here -->

## Notes for reviewers

<!-- Anything reviewers should pay special attention to -->