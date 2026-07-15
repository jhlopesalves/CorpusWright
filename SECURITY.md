# Security Policy

## Supported versions

Security fixes are provided for the latest published CorpusWright alpha release and the current
`main` branch. Older alpha builds may require upgrading because project and cache formats are still
evolving.

## Reporting a vulnerability

Please report suspected vulnerabilities privately through
[GitHub Security Advisories](https://github.com/jhlopesalves/CorpusWright/security/advisories/new).
If the advisory form is unavailable, email the maintainer at
[jhlopesalves@gmail.com](mailto:jhlopesalves@gmail.com).
Do not open a public issue for an unpatched vulnerability.

Include the affected version or commit, operating system, a minimal reproduction using synthetic
or otherwise shareable data, observed impact, and any relevant logs with personal or research data
removed. The maintainer will acknowledge the report when available, investigate it, and coordinate
disclosure and credit with the reporter. No fixed response-time guarantee is offered for this
maintainer-run experimental project.

## Threat model

CorpusWright treats imported TXT, HTML, DOCX, PDF, project metadata, extraction caches, annotation
caches, and Concordance indexes as untrusted input. Native PDFium, OCR, and UDPipe components are
also security boundaries. Reports involving malformed documents, archive/XML amplification, path
traversal, unsafe native output, resource exhaustion, or data written outside a selected project or
export directory are in scope.

CorpusWright is local-first and does not provide a multi-user service or sandbox hostile documents.
Only open files you are authorised to access, keep the application and operating system updated,
and avoid processing suspicious documents until a reported issue is fixed.

Please allow a reasonable private remediation period before public disclosure. If a report is not
security-sensitive, it may be redirected to the public issue tracker after consulting the reporter.
