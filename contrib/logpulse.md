# logpulse

> High-performance TUI log analyzer with smart format auto-detection.
> Supports JSON, Laravel, Django, Go, Nginx out of the box.
> More information: <https://github.com/vltamanec/logpulse>.

- Monitor a local log file:

`logpulse {{path/to/file.log}}`

- Monitor multiple files at once:

`logpulse {{app.log}} {{nginx.log}} {{error.log}}`

- Force a specific log format:

`logpulse --format {{json|laravel|django|go|nginx|plain}} {{path/to/file.log}}`

- Pipe logs from stdin:

`docker logs -f {{container}} 2>&1 | logpulse`

- Monitor Docker container (smart prefix match + auto-reconnect):

`logpulse docker {{container_prefix}}`

- Monitor a log file inside a Docker container:

`logpulse docker {{container_prefix}} {{/var/log/app.log}}`

- Monitor remote file via SSH:

`logpulse ssh {{user@host}} {{/var/log/app.log}}`

- Monitor remote Docker container via SSH:

`logpulse ssh {{user@host}} docker {{container_prefix}}`

- SSH via jump host / proxy (no ssh config needed):

`logpulse ssh {{user@host}} -J {{bastion.corp.com}} docker {{container_prefix}}`

- SSH with custom port and key:

`logpulse ssh {{user@host}} -p {{2222}} -i {{~/.ssh/id_ed25519}} {{/var/log/app.log}}`

- Monitor Kubernetes pod logs:

`logpulse k8s {{pod_name}} -n {{namespace}}`

- Find Kubernetes pod by label:

`logpulse k8s -l {{app=api}} -n {{prod}}`

- Monitor Docker Compose service:

`logpulse compose {{service_name}}`

- Monitor a file inside a Kubernetes pod:

`logpulse k8s {{pod_name}} {{/var/log/app.log}}`

- Generate shell completions:

`logpulse --completions {{bash|zsh|fish}}`

- Interactive hotkeys inside TUI:

`? search, n/N navigate, * highlight, y copy, s save, g time-jump, / filter, e errors`
