output "kubeconfig_path" {
  description = "Path to the generated kubeconfig file"
  value       = "${path.module}/kubeconfig"
}
