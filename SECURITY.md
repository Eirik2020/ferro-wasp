# Security

FerroWasp is an experimental prototype and currently has no production
security guarantee.

## Reporting

Use GitHub's private vulnerability-reporting form:

<https://github.com/Eirik2020/ferro-wasp/security/advisories/new>

Sign in to GitHub, select **Report a vulnerability**, and include the affected
revision, impact, reproduction conditions, and the least hazardous evidence
needed to understand the issue. Do not include live credentials or unnecessarily
detailed motor-test instructions.

If the private form is unavailable, open a public issue that contains only a
request for a private reporting channel. Do not include the vulnerability
details in that issue.

Do not publish exploit details, unsafe motor-output bypasses, or hardware-risk
instructions before the issue has been acknowledged and a coordinated
disclosure plan has been agreed.

## Scope

Security reports may include:

- paths that bypass arming or actuator gating;
- telemetry/config paths that can mutate safety state;
- unsafe default behavior;
- denial-of-service paths in control, RC, IMU, or actuator tasks;
- malformed protocol frames that affect safety state;
- secrets or private data accidentally committed to the repository.

## Supported Versions

No production versions are currently supported. Public code should be treated as
experimental bench software only.
