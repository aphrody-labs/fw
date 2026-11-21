---
name: better-auth
description: "Better Auth in TypeScript apps: scaffold and configure server and client, adapters, sessions, plugins. Use when the user mentions Better Auth, auth.ts, login, sign-up, MFA or multi-tenant auth."
---
> email/password and verification; organizations and RBAC; two-factor and MFA; security hardening (rate limits, CSRF, trusted origins, secrets, audit).

# Better Auth

Scaffold with `create-auth`, then the reference for the feature being configured.

Read the reference that matches the task in full before acting. Paths written inside reference files are relative to this skill's directory (the one holding this SKILL.md). Where a reference names another merged skill, open `references/<name>.md`; its helper files are under `references/<name>/`.

| Reference | Use when |
| --- | --- |
| [references/core.md](references/core.md) | Former body of this skill (read first) |
| [references/create-auth.md](references/create-auth.md) | Scaffold and implement authentication in TypeScript/JavaScript apps using Better Auth. Detect frameworks, configure database adapters, set up route handlers, add OAuth providers, and create auth UI pages. Use when users want to add login, sign-up, or authentication to a new or existing project with Better Auth. |
| [references/better-auth-best-practices.md](references/better-auth-best-practices.md) | Configure Better Auth server and client, set up database adapters, manage sessions, add plugins, and handle environment variables. Use when users mention Better Auth, betterauth, auth.ts, or need to set up TypeScript authentication with email/password, OAuth, or plugin configuration. |
| [references/email-and-password-best-practices.md](references/email-and-password-best-practices.md) | Configure email verification, implement password reset flows, set password policies, and customise hashing algorithms for Better Auth email/password authentication. Use when users need to set up login, sign-in, sign-up, credential authentication, or password security with Better Auth. |
| [references/organization-best-practices.md](references/organization-best-practices.md) | Configure multi-tenant organizations, manage members and invitations, define custom roles and permissions, set up teams, and implement RBAC using Better Auth's organization plugin. Use when users need org setup, team management, member roles, access control, or the Better Auth organization plugin. |
| [references/two-factor-authentication-best-practices.md](references/two-factor-authentication-best-practices.md) | Configure TOTP authenticator apps, send OTP codes via email/SMS, manage backup codes, handle trusted devices, and implement 2FA sign-in flows using Better Auth's twoFactor plugin. Use when users need MFA, multi-factor authentication, authenticator setup, or login security with Better Auth. |
| [references/better-auth-security-best-practices.md](references/better-auth-security-best-practices.md) | Configure rate limiting, manage auth secrets, set up CSRF protection, define trusted origins, secure sessions and cookies, encrypt OAuth tokens, track IP addresses, and implement audit logging for Better Auth. Use when users need to secure their auth setup, prevent brute force attacks, or harden a Better Auth... |

<!-- merge-skill-families:v1 -->
