# Security policy

## Supported versions

The current release is 0.1.0. Fixes go into the newest code on `main`, and
the next release carries them.

## Reporting a vulnerability

Use GitHub's private vulnerability reporting on this repository (**Security
→ Report a vulnerability**). Do not open a public issue for a security
problem. Include the commit, `cig --version`, the platform, and the file or
command that shows the problem. You'll get an acknowledgement within a few
days and a fix or a mitigation before anything is disclosed.

## What CigarScript promises and what it does not

- **The burn rule holds at every strength.** Nothing touches the world outside
  a `burn { }`, and full strength's foreign calls are journaled as
  irreversible unless they come with an adapter. A program that gets around
  the rule is a security bug; report it here.
- **The checker is the first reader of untrusted input.** It is cig's own
  lexer, parser and checker underneath, with CigarScript's strength gate in
  front. A file that crashes it, or that it accepts at a strength that should
  refuse it, is a bug worth reporting.
- **A script is code.** A CigarScript program runs as you, with your
  permissions. Read code from someone else before you run it.
