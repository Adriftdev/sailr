provider "minikube" {
  kubernetes_version = "{{kube_version}}"
}

provider "docker" {
  host = "tcp://localhost:2375"
}

resource "docker_image" "registry" {
  name = "registry:2.10.1-stable"
}

resource "docker_container" "registry" {
  name  = "local-docker-registry"
  image = docker_image.registry.latest
  ports {
    internal = 5000
    external = 5000
  }
}

resource "minikube_cluster" "docker" {
  driver       = "docker"
  cluster_name = "{{cluster_name}}"
  addons = [
    "default-storageclass",
    "storage-provisioner",
  ]
}

provider "kubernetes" {
  host = minikube_cluster.docker.host

  client_certificate     = minikube_cluster.docker.client_certificate
  client_key             = minikube_cluster.docker.client_key
  cluster_ca_certificate = minikube_cluster.docker.cluster_ca_certificate
}
