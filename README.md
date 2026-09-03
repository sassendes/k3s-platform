# k3s-platform

A small managed-hosting platform on a two-node Kubernetes cluster. It runs a real
application with a database, watches itself, backs itself up.
## What it is

Bare-metal Proxmox host, two Debian VMs, k3s across both: one control-plane, one
worker. Everything above that runs as pods the scheduler places across the two nodes,
so the loss of the worker is a scheduling event, not an outage. Its pods reschedule
onto the control-plane and the app stays up.

The workload is a veterinary-clinic records API, a stand-in for the kind of
line-of-business app a small shop actually pays to have hosted. It is a Rust service
that talks to PostgreSQL, and it exists so the platform has something real to host,
monitor, back up, and defend.

## What it does

- Two-node k3s cluster, control-plane plus worker, all state in version-controlled YAML.
- Rust/Axum API, two replicas behind a Service, so a dead pod is replaced without a dropped request.
- PostgreSQL as its own pod with a persistent volume, reached only by service name.
- Self-healing: kill a pod and the deployment rebuilds it; kill the worker and its pods move to the control-plane.
- Prometheus scraping per-pod CPU and memory off each node's cAdvisor, RBAC scoped to exactly what it needs.
- Grafana for the live cluster view, plus a second data source straight into Postgres for the application's own numbers.
- Alertmanager rules that fire when a replica disappears or a pod runs hot, evaluated continuously.
- Nightly backup CronJob: dump, verify the dump is real before trusting it, rotate so a corrupt backup can never overwrite a good one.
- A restore path that has actually been run: database wiped, restored from the latest dump, row count back where it started.
- Default-deny network policy across the namespace, explicit allows only for the pods that legitimately take traffic.
- The database accepts connections from the app pod and nothing else.
- Credentials live in Secrets, never in the image, never in a committed file.
- TLS on the app-to-database connection, unreadable even to something already inside the pod network.

## Roadmap

The two-node design covers workload failover today. The next moves are known and
scoped:

- Control-plane HA. A three-node control plane removes the last single point of failure. The workload already survives a node loss; the cluster brain does not yet.
- External ingress with a real certificate. Reachable on the LAN now; a proper front door is deliberately the last layer, added once everything behind it is locked down.
- Off-cluster backups. Verification and rotation are done; shipping a verified copy to a second location and encrypting it at rest is designed and next in line.
- A shared image registry, replacing the per-node image import that stands in for it at this scale.

## Why the backup ordering matters


## Layout

    Cargo.toml, src/       the Rust API
    Dockerfile             multi-stage build, ~87MB runtime image
    index.html             the frontend, served by the app itself
    postgres-*.yaml        database: secret, storage, deployment, service
    app-deployment.yaml    the app: deployment and service, creds from secrets
    prometheus.yaml        RBAC, scrape config, alert rules
    grafana.yaml           dashboards
    backup-cronjob.yaml    nightly dump, verify, rotate
    restore.yaml           the tested restore path
    default-deny.yaml      deny all traffic by default
    allow-policies.yaml    explicit allows for what needs reaching
    db-networkpolicy.yaml  only the app reaches the database
    ingress.yaml           external routing

Secrets and TLS keys are gitignored on purpose. The repo describes how to provide
them; it does not carry them.

## License

MIT
