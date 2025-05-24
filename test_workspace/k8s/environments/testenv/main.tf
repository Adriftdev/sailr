resource "null_resource" "install_k3s" {
  provisioner "local-exec" {
    command = <<EOF
      curl -sfL https://get.k3s.io | INSTALL_K3S_VERSION="${var.k3s_version}" sh -s - server \
        --disable=traefik \
        --write-kubeconfig-mode 644 \
        --cluster-init ${var.worker_node_count > 0 ? "" : "--cluster-init"}
    EOF

    interpreter = ["bash", "-c"]
    when        = create  # No quotes around create
  }

  # Use provisioner triggers to simulate depends_on
  triggers = {
    install_k3s = timestamp()
  }

  # If worker nodes are desired, add them
  provisioner "local-exec" {
    command = <<EOF
      export K3S_TOKEN=$(sudo cat /var/lib/rancher/k3s/server/node-token)
      for i in $(seq 1 ${var.worker_node_count}); do
        curl -sfL https://get.k3s.io | K3S_URL=https://127.0.0.1:6443 K3S_TOKEN=$K3S_TOKEN INSTALL_K3S_VERSION="${var.k3s_version}" sh -
      done
    EOF

    interpreter = ["bash", "-c"]
    when        = create # No quotes
  }

  # Copy the kubeconfig to the module directory
  provisioner "local-exec" {
    command = "cp /etc/rancher/k3s/k3s.yaml ${path.module}/kubeconfig"
    
    interpreter = ["bash", "-c"]
    when        = create # No quotes
  }

  # Destroy the cluster by uninstalling k3s
  provisioner "local-exec" {
    command = "sudo /usr/local/bin/k3s-uninstall.sh"
    when    = destroy # No quotes
  }
}


# Format the GCP credentials file to be a single line
resource "local_file" "formatted_gcp_credentials" {
  content = jsonencode(jsondecode(file(var.gcp_credentials_file)))
  filename = "${path.module}/gcp_credentials_oneline.json"
}

# Create the registries.yaml file
resource "local_file" "registries_yaml" {
  content = templatefile("${path.module}/registries.tmpl", {
    gcp_project_id = var.gcp_project_id
    gcp_region     = var.gcp_region
    gcp_credentials = local_file.formatted_gcp_credentials.content
  })
  filename = "${path.module}/registries.yaml"
}

# Copy the registries.yaml file to the k3s server
resource "null_resource" "copy_registries_config" {
  provisioner "local-exec" {
    command = "sudo cp ${local_file.registries_yaml.filename} /etc/rancher/k3s/registries.yaml"

    interpreter = ["bash", "-c"]
    when = create
  }

  triggers = {
    registries_yaml_content = local_file.registries_yaml.content
  }

  depends_on = [
    null_resource.install_k3s
  ]
}

# Restart k3s to pick up the new registries.yaml file
resource "null_resource" "restart_k3s" {
  provisioner "local-exec" {
    command = "sudo systemctl restart k3s"

    interpreter = ["bash", "-c"]
    when = create
  }

  triggers = {
    always_run = timestamp()
  }

  depends_on = [
    null_resource.copy_registries_config
  ]
}
