### Sailr: The Calming Force in the Choppy Waters of Kubernetes

Kubernetes is a powerful tool for managing containerized applications, 
but it can also be complex and challenging to use. If you're feeling overwhelmed by 
Kubernetes, Sailr can help.

Sailr is an environment management CLI that makes it easy to deploy, manage, and troubleshoot 
Kubernetes applications. With Sailr, you can:

- Automate deployments and updates so you can sail through your work.
- Manage resources efficiently so you don't run aground.
- Troubleshoot problems quickly and easily so you can stay afloat.

Sailr is the perfect tool for Kubernetes users who want to save time, reduce stress, 
and get more out of their Kubernetes deployments.

Try Sailr today and see the difference it can make.

### System Requirments

- OpenTofu (Terraform replacement).
- Docker

### Usage 

#### Builtin Template Environment Variables 

- name - The name of the environment.
- log_level - the global logging leveling, generally used for setting the log level of the applications
- domain - The domain url
- default_replicas - The global environment level default replicas setting. 

* Custom environment variables can be set by using the following 

```toml
[[environment_variables]]
name = "DB_HOST"
value = "postgres"

[[environment_variables]]
name = "DB_PORT"
value = "5432"
```

#### Service Whitelist

Service whitelist defines the services that are allowed to run in a particular
environment. They are defined using the following 

```toml
[[service_whitelist]]
name = "core/proxy"
version = "1.0.0"

[[service_whitelist]]
path="postgres"
name = "postgres"
version = "1.0.0"

```
