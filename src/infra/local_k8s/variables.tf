variable "k3s_version" {
  description = "Version of k3s to install (e.g., v1.27.1+k3s1)"
  type        = string
  default     = "v1.27.1+k3s1"
}

variable "worker_node_count" {
  description = "Number of worker nodes to add to the k3s cluster"
  type        = number
  default     = 0
}

variable "gcp_region" {
  description = "Google Cloud region"
  type        = string
  default     = "europe-west2"
}

variable "gcp_project_id" {
  description = "Google Cloud project ID for Artifact Registry"
  type        = string
}

variable "gcp_credentials_file" {
  description = "Path to the GCP service account credentials JSON file"
  type        = string
}
