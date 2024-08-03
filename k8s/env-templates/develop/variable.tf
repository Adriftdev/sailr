variable "region" {
  default = "{{region}}"
}

variable "project" {
  default = "{{project_id}}"
}

variable "zone" {
  default = "{{zone}}"
}

# Number of worker nodes
# 2vcpu, 8GB RAM per node
variable "worker_nums" {
  default = "{{worker_nums}}"
}
