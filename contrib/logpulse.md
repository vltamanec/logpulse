# logpulse

> High-performance TUI log analyzer with smart format auto-detection.
> Supports Laravel, Django, Go, Nginx, JSON out of the box.
> More information: <https://github.com/vltamanec/logpulse>.

- Monitor a local log file:

`logpulse {{path/to/file.log}}`

- Read logs from stdin (e.g. Docker):

`docker logs -f {{container}} 2>&1 | logpulse -`

- Monitor Docker container stdout:

`logpulse docker {{container}}`

- Monitor a log file inside a Docker container:

`logpulse docker {{container}} {{/var/log/app.log}}`

- Show version:

`logpulse --version`
