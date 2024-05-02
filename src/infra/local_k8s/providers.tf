terraform {
  required_providers {
    minikube = {
      source = "scott-the-programmer/minikube"
      version = "0.3.10"
    }
    docker = {
      source  = "kreuzwerker/docker"
      version = "~> 2.23"
    }
  }
}
