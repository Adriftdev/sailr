variable "k3s_version" {
  description = "Version of k3s to install (e.g., v1.27.1+k3s1)"
  type        = string
  default     = "v1.27.1+k3s1" # Or latest, if you prefer
}

variable "worker_node_count" {
  description = "Number of worker nodes to add to the k3s cluster"
  type        = number
  default     = 0
}
