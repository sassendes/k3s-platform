# k3s-platform

A small managed-hosting platform on a two-node Kubernetes cluster. It runs a
real application with a database, watches itself, backs itself up, and heals
itself when something dies. You declare the state you want in YAML and the
cluster spends the rest of its life making reality match.

It is built on the same idea every large infrastructure runs on: reconciliation.
Google open-sourced Kubernetes in 2014 out of their internal Borg system, and the
core of it is a control loop that continuously drives the cluster toward a
declared desired state. Nothing here is clicked into place by hand. Every
component is a file, and the files are the system.

## What it is

Bare-metal Proxmox host, two Debian VMs, k3s across both. One control-plane, one
worker. Everything above that runs as pods the scheduler places across the two
nodes, so the loss of a node is a scheduling event, not an outage.

The workload is a veterinary-clinic records API, a stand-in for the kind of
line-of-business app a small shop actually pays to have hosted. It is a Rust
service that talks to PostgreSQL, and it exists so the platform has something
real to host, monitor, back up, and defend.

## Working

- Two-node k3s cluster, control-plane plus worker, all state in version-controlled YAML.
- Rust/Axum API, two replicas, behind a Service so a dead pod is replaced without a dropped request.
- PostgreSQL as a separate pod with its own persistent volume, reached only by name.
- Self-healing: kill a pod and the deployment rebuilds it; kill a node and its pods reschedule.
- Prometheus scraping per-pod CPU and memory off each node's cAdvisor, with RBAC scoped to exactly what it needs.
- Grafana on top for the live view, plus a second data source straight into Postgres for the application's own numbers.
- Alertmanager rules that fire when a replica disappears or a pod's memory runs hot, evaluated continuously, not on a cron.
- Nightly backup CronJob: dump, verify the dump is real before trusting it, and rotate so a corrupt backup can never overwrite a good one.
- A restore path that has actually been run: database wiped, restored from the latest dump, row count back to where it started.
- Default-deny network policy across the namespace, with explicit allows only for the pods that legitimately take traffic.
- The database accepts connections from the app pod and nothing else.
- Credentials live in Secrets, never in the image and never in a committed file.
- TLS on the app-to-database connection, so the traffic is unreadable even to something already inside the pod network.

## Not there yet

- Control-plane HA. One control-plane node means one brain; real high availability wants three. The worker failover works today, the control plane is a single point.
- External ingress with public TLS. Reachable on the LAN through a NodePort; a real front door with a certificate is the next step and is deliberately last.
- CI/CD. Images are built and imported by hand. At two nodes and one app the pipeline would be ceremony, so it is noted, not built.
- A shared registry. Images are imported to each node individually, which is the honest small-cluster version of what a registry does properly.
- Off-cluster backup copies and encryption at rest. The rotation and verification are done; shipping a copy to a second location is designed and not yet wired.

## Why the backup ordering matters

The rule is that a bad backup must never cost you a good one. So the job dumps
the database, then checks the dump is non-empty and structurally real, and only
if that passes does it delete the oldest copy to make room. A failure anywhere
before the check exits without touching the existing backups. The verification
runs before the rotation, never after, and that ordering is the whole guarantee.

## Layout
