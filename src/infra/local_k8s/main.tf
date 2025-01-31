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
