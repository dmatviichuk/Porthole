# Security policy

## Supported versions

Security fixes go into the next release only. Check that a problem still happens on the
[latest release](https://github.com/dmatviichuk/Porthole/releases/latest) before you report it.

## Reporting a vulnerability

Please don't open a public issue. Report it privately on GitHub instead:
[Report a vulnerability](https://github.com/dmatviichuk/Porthole/security/advisories/new) (the
**Security** tab, then **Report a vulnerability**).

Include:

- the Porthole version and your operating system;
- what an attacker could do, and what they need first (a malicious cluster, a crafted resource,
  access to your machine);
- steps to reproduce or a proof of concept;
- a suggested fix, if you have one.

Never send real credentials, kubeconfigs or Secret values; make up stand-ins.

You'll get a reply within a week. Once the problem is confirmed, the fix ships in a release, then
the advisory is published, crediting you unless you'd rather stay anonymous. Please keep the details
private until then.

## Scope

Porthole acts with the credentials in your kubeconfig and whatever they are allowed to do. A
vulnerability is anything that makes it act beyond what you asked, or leaks what it holds. For
example:

- Porthole creating a resource, saving YAML over an object other than the one it opened, or
  saving without the `resourceVersion` check.
- A delete skipping the typed-name confirmation on a namespace or on a cluster labelled to ask.
- Tokens, client keys, exec plugin output or Secret values ending up in the log, in the local cache
  or anywhere other than the cluster's API server.
- Data from a cluster (resource fields, labels, events, log lines, YAML) running script in the app
  or calling its backend commands.
- A weakness in the release workflow that would let someone else's code ship as Porthole.

Out of scope:

- Anything your kubeconfig's credentials are already allowed to do. Limit that with RBAC.
- Vulnerabilities in Kubernetes, in exec credential plugins (`aws`, `kubelogin`...) or in the
  operating system's web view. Report those to their projects.
- Vulnerabilities in a dependency that Porthole doesn't reach. If it does reach the vulnerable code,
  report it here too.
- The builds not being code-signed yet. This is known and covered in the
  [README](../README.md#install).
- Attacks that need control of your user account or machine already.
