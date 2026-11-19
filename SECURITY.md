# Security

anyzoom installs a component (`anyzoom_vcam.dll`) that runs inside the
Windows camera service, and its installer asks for administrator rights. We
treat security problems in it as the most urgent kind of bug.

## Reporting a problem

Please do not open a public issue for a security problem.

Email **3e@eeestudios.com** with `anyzoom security` in the subject. Include:

- the anyzoom version (shown in the window, or the release zip name)
- your Windows 11 build
- what you did, what happened, and what an attacker could gain from it

We aim to reply within 7 days and tell you what we plan to do. When a fix
ships, we credit you in the release notes unless you ask us not to.

## Supported versions

Only the latest release gets security fixes.

## In scope

- The installer and uninstaller scripts
- The virtual camera component and the shared frame file it reads
- The anyzoom app

Problems in Windows itself, in webcam drivers or in the apps that use
anyzoom Camera belong with those vendors.
