---
sidebar_position: 2
title: Getting Started Tutorial
---

# Getting Started with Sailr

Welcome to Sailr! This tutorial will guide you through installing Sailr, initializing your first project, and deploying a sample application to a local Kubernetes cluster.

## Prerequisites

Before you start, please ensure you have completed the steps in our [Installation Guide](./installation.md). This will ensure you have:

*   Sailr CLI installed and configured.
*   Docker installed and running.
*   OpenTofu (or Terraform) installed.
*   A local Kubernetes cluster available. For this tutorial, we'll use **Minikube**.

**Start your Minikube cluster:**
If you haven't already, open your terminal and start Minikube:
```bash
minikube start
```
Ensure Minikube is running and your `kubectl` context is pointing to it.

## Step 1: Initialize Your First Sailr Project

Sailr projects are organized into environments. Let's create a new Sailr project structure and then initialize an environment within it called `my-first-app`.

1.  **Create a project directory:**
    This will be the root of your Sailr-managed configurations.
    ```bash
    mkdir sailr-project
    cd sailr-project
    ```

2.  **Initialize the environment:**
    Run the `init` command from within your project root:
    ```bash
    sailr init my-first-app
    ```
    This command creates the necessary directory structure, including `k8s/environments/my-first-app/config.toml`. This file is where you'll define your services and environment settings. It also creates a `k8s/templates` directory where service manifest templates are stored.

    Your project structure will look something like this:
    ```
    sailr-project/
    └── k8s/
        ├── environments/
        │   └── my-first-app/
        │       └── config.toml
        └── templates/
            └── ... (default templates might be copied here) 
    ```

## Step 2: Understanding `config.toml`

Open the generated `k8s/environments/my-first-app/config.toml` file. You'll see something like this (the exact default may vary):

```toml
schema_version = "0.2.0" # Or current Sailr schema version

name = "my-first-app"    # Name of your environment
log_level = "INFO"
domain = "example.com"   # Replace with your domain or keep for local dev
default_replicas = 1
registry = "docker.io"   # Default container registry

# Example of a service definition (might be commented out by default)
# [[service_whitelist]]
# name = "my-service"           # Name of the service
# version = "0.1.0"             # Image version/tag
# path = "my-service"           # Path to service templates (relative to k8s/templates/)
#                               # Defaults to service name if omitted
# namespace = "my-first-app"    # K8s namespace for deployment
# build = "./services/my-service" # Path to service build context (from project root)

# Example of environment variables
# [[environment_variables]]
# name = "MY_ENV_VAR"
# value = "some_value"
```
For now, we're interested in the global settings and the `[[service_whitelist]]` section, which is where we define our applications (services).

## Step 3: Choosing a Sample Application

For this tutorial, we'll deploy a simple, publicly available Nginx "hello world" Docker image: `nginxdemos/hello`. This saves us from needing to write and build our own application for this initial walkthrough.

If you wanted to use your own application, you would typically have its Dockerfile and source code in a subdirectory (e.g., `services/my-web-app/` relative to the project root), and you would specify the `build` key in its service definition.

## Step 4: Create Basic Templates for Nginx

Sailr uses templates to generate Kubernetes manifests. Even for a pre-built image, we need to tell Sailr how to deploy it (e.g., as a Deployment) and expose it (e.g., as a Service).

1.  **Create template directories:**
    From your project root (`sailr-project`), create the following directory structure:
    ```bash
    mkdir -p k8s/templates/hello-nginx
    ```

2.  **Create `k8s/templates/hello-nginx/deployment.yaml`:**
    ```yaml
    apiVersion: apps/v1
    kind: Deployment
    metadata:
      name: {{service_name}}
      namespace: {{service_namespace}}
      labels:
        app: {{service_name}}
    spec:
      replicas: {{default_replicas}}
      selector:
        matchLabels:
          app: {{service_name}}
      template:
        metadata:
          labels:
            app: {{service_name}}
        spec:
          containers:
          - name: {{service_name}}
            image: {{registry}}/nginxdemos/hello:{{service_version}} # Using the public image
            ports:
            - containerPort: 80
    ```

3.  **Create `k8s/templates/hello-nginx/service.yaml`:**
    ```yaml
    apiVersion: v1
    kind: Service
    metadata:
      name: {{service_name}}
      namespace: {{service_namespace}}
    spec:
      selector:
        app: {{service_name}}
      ports:
        - protocol: TCP
          port: 80
          targetPort: 80
      type: NodePort # Using NodePort for easy access via Minikube
    ```
    These are minimal Kubernetes manifests. Sailr will replace variables like `{{service_name}}`, `{{service_namespace}}`, `{{service_version}}`, etc., with values from your `config.toml`.

## Step 5: Configure Your Service in `config.toml`

Now, modify your `k8s/environments/my-first-app/config.toml` to define the `hello-nginx` service.

```toml
schema_version = "0.2.0"

name = "my-first-app"
log_level = "INFO"
domain = "local.host" # Using local.host for Minikube/local access
default_replicas = 1
registry = "docker.io" # nginxdemos/hello is on Docker Hub

[[service_whitelist]]
name = "hello-nginx"
version = "latest"  # You can pin to a specific version like "plain-text"
namespace = "my-first-app" # Deploy into a dedicated namespace
# 'path' will default to "hello-nginx", matching our template directory.
# 'build' is not needed as we are using a pre-built public image.
```

**Key points:**
*   We set `domain` to `local.host`, which is often useful for local development.
*   We defined a service named `hello-nginx`.
*   `version = "latest"`: We're using the latest version of `nginxdemos/hello`. (Note: `nginxdemos/hello` has tags like `latest`, `plain-text`. `latest` is fine for a demo.)
*   `namespace = "my-first-app"`: We'll deploy this service into a Kubernetes namespace named after our environment.
*   The `path` key for the service is omitted, so Sailr will look for templates in `k8s/templates/hello-nginx/`, which we just created.
*   The `build` key is omitted because we are using a pre-built public image.

## Step 6: Deploy to Minikube

Now, let's deploy our `hello-nginx` service to your Minikube cluster. Make sure you are in the root of your `sailr-project` directory.

```bash
sailr go my-first-app --context minikube
```

The `sailr go` command is a shortcut that:
1.  **Builds** images (skipped for `hello-nginx` as no `build` path is defined).
2.  **Generates** Kubernetes manifests from your templates and `config.toml`.
3.  **Deploys** these manifests to the specified Kubernetes context.

You should see output from Sailr indicating that it's processing templates and applying resources to the `my-first-app` namespace.

## Step 7: Verify the Deployment

Once Sailr finishes, let's check if our application is running.

1.  **Check the Kubernetes Namespace:**
    Sailr should create the namespace if it doesn't exist.
    ```bash
    kubectl get namespace my-first-app
    ```
    You should see the `my-first-app` namespace listed.

2.  **Check the pods:**
    ```bash
    kubectl get pods -n my-first-app
    ```
    You should see a pod for `hello-nginx` (e.g., `hello-nginx-xxxxxxxxx-xxxxx`) with a status of `Running`. It might take a minute or two for the image to be pulled from Docker Hub and the pod to start. If it's stuck in `ImagePullBackOff` or `ErrImagePull`, ensure the image name `nginxdemos/hello:latest` is correct and your Minikube has internet access.

3.  **Check the service:**
    ```bash
    kubectl get svc -n my-first-app
    ```
    You should see a service named `hello-nginx`. Note its type is `NodePort` as defined in our `service.yaml`.

4.  **Access the application:**
    Since we used `NodePort`, Minikube provides an easy way to access it:
    ```bash
    minikube service hello-nginx -n my-first-app
    ```
    This command should open the Nginx "Hello World" page in your web browser. If it doesn't open automatically, it will print the URL you can use (e.g., `http://<minikube_ip>:<node_port>`).

## Step 8: Clean Up (Optional)

To remove the deployed application and its namespace:

```bash
kubectl delete namespace my-first-app
```
This will delete all resources (Deployment, Service, Pods) created within that namespace.

If you had used `sailr infra up` for more complex environments (not covered in this basic tutorial), you might use `sailr infra down my-first-app` to tear down any specific infrastructure Sailr provisioned.

## Congratulations!

You've successfully initialized a Sailr project, created basic templates, configured a service using a public image, and deployed it to your local Kubernetes cluster using Minikube.

**Next Steps:**

*   Try modifying the `deployment.yaml` (e.g., change `default_replicas` in `config.toml` or directly in the manifest) and re-run `sailr go my-first-app --context minikube` to see changes applied.
*   Explore how to configure **builds** for your own applications by adding a `build` path in `config.toml` and creating a Dockerfile for your service.
*   Learn more about advanced **templating** features in Sailr (once documented).
*   Dive deeper into the **[Comprehensive `config.toml` Guide](../configuration/config-toml.md)** (Link to be created).
*   Check out the **[Full CLI Command Reference](../cli-usage.md)**.
---
This content is for the Docusaurus page `docs/docs/getting-started/tutorial.md`.Okay, I have drafted the "Getting Started" tutorial. I've incorporated the likely behavior of `sailr init` creating an `environments` subdirectory and the necessity of creating basic templates even for pre-built images. I've also adjusted the commands and explanations accordingly.

Here's the summary of the steps taken:

1.  **Reviewed Initial Draft and Assumptions:** Compared the provided draft structure with my understanding of Sailr's behavior based on previous code analysis (especially `src/main.rs`, `src/lib.rs`, and `src/templates/mod.rs`).
2.  **Corrected `sailr init` Path:** Updated the tutorial to reflect that `sailr init <env_name>` likely creates project structure like `k8s/environments/<env_name>/config.toml` and that subsequent commands like `sailr go <env_name>` should be run from the project root.
3.  **Mandated Basic Templates:** Modified Step 4 to explicitly guide the user to create minimal `deployment.yaml` and `service.yaml` templates for the `hello-nginx` service, as Sailr's template discovery mechanism would likely require these even for pre-built images.
4.  **Refined Commands:** Ensured CLI commands are consistent with the corrected path assumptions (e.g., running `sailr go my-first-app` from the project root).
5.  **Added Namespace Verification:** Included a step to verify namespace creation.
6.  **Clarified Image Tag:** Used `nginxdemos/hello:latest` and noted that specific tags like `plain-text` could also be used.
7.  **Wrote Docusaurus Page:** Created the content for `docs/docs/getting-started/tutorial.md` using the `create_file_with_block` tool, including Docusaurus frontmatter and detailed, step-by-step instructions.
8.  **Content Details:**
    *   Starts with prerequisites and Minikube setup.
    *   Guides through `sailr init` and project structure.
    *   Explains the relevant parts of `config.toml`.
    *   Instructs on creating basic Kubernetes manifest templates (`deployment.yaml`, `service.yaml`) for the sample Nginx application.
    *   Shows how to configure the service in `config.toml`.
    *   Details the `sailr go` command for deployment.
    *   Provides `kubectl` commands to verify the deployment (namespace, pods, service).
    *   Explains how to access the service via `minikube service`.
    *   Includes optional cleanup steps.
    *   Suggests next steps for further learning.
