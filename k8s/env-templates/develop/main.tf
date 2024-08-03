provider "google" {
  project = var.google_project_id
  region  = var.region
  zone    = var.az
}

# 2vcpu, 8GB RAM
resource "google_compute_instance" "k3s_master_instance" {
  name         = "k3s-master"
  machine_type = "n2d-standard-2"
  tags         = ["k3s", "k3s-master", "http-server", "https-server"]

  boot_disk {
    initialize_params {
      image = "debian-9-stretch-v20200805"
    }
  }

  network_interface {
    network = "default"

    access_config {}
  }

  provisioner "local-exec" {
    command = <<EOT
            k3sup install \
            --ip ${self.network_interface[0].access_config[0].nat_ip} \
            --context k3s \
            --ssh-key ~/.ssh/google_compute_engine \
            --user $(whoami) \
            --k3s-extra-args '--no-deploy -traefik'
        EOT
  }

  depends_on = [
    google_compute_firewall.k3s-firewall,
  ]
}

# 2vcpu, 8GB RAM
resource "google_compute_instance" "k3s_worker_instance" {
  count        = var.worker_nums
  name         = "k3s-worker-${count.index}"
  machine_type = "n2d-standard-2"
  tags         = ["k3s"]

  boot_disk {
    initialize_params {
      image = "debian-9-stretch-v20200805"
    }
  }

  network_interface {
    network = "default"

    access_config {}
  }

  provisioner "local-exec" {
    command = <<EOT
            k3sup join \
            --ip ${self.network_interface[0].access_config[0].nat_ip} \
            --server-ip ${google_compute_instance.k3s_master_instance.network_interface[0].access_config[0].nat_ip} \
            --ssh-key ~/.ssh/google_compute_engine \
            --user $(whoami)
        EOT
  }

  depends_on = [
    google_compute_firewall.k3s-firewall,
  ]
}

resource "google_compute_firewall" "k3s-firewall" {
  name    = "k3s-firewall"
  network = "default"

  allow {
    protocol = "tcp"
    ports    = ["6443"]
  }

  target_tags = ["k3s"]
}

resource "google_dns_managed_zone" "k3s_zone" {
  name        = "k3s_zone"
  dns_name    = "skyfleet.adriftdev.com."
  description = "k3s DNS zone"
}

resource "google_dns_record_set" "a" {
  name         = google_dns_managed_zone.k3s_zone.dns_name
  managed_zone = google_dns_managed_zone.k3s_zone.name
  type         = "A"
  ttl          = 300

  rrdatas = [google_compute_instance.k3s_master_instance.network_interface[0].access_config[0].nat_ip]
}

output "name_servers" {
  value = google_dns_managed_zone.k3s_zone.name_servers
}
