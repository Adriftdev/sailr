// src/templates/scaffolding.rs

pub fn generate_deployment(
    service_name: &str,
    _app_type: &str,
    image: &str,
    replicas: u8,
    port: u16,
) -> String {
    format!(
        r#"apiVersion: apps/v1
kind: Deployment
metadata:
  name: {service_name}
  labels:
    app: {service_name}
spec:
  replicas: {replicas}
  selector:
    matchLabels:
      app: {service_name}
  template:
    metadata:
      labels:
        app: {service_name}
    spec:
      containers:
      - name: {service_name}
        image: {image}
        ports:
        - containerPort: {port} # Default, consider making this configurable later
"#,
        service_name = service_name,
        replicas = replicas,
        image = image,
        port = port
    )
}

pub fn generate_service(service_name: &str, _app_type: &str, port: u16) -> String {
    format!(
        r#"apiVersion: v1
kind: Service
metadata:
  name: {service_name}
spec:
  selector:
    app: {service_name}
  ports:
    - protocol: TCP
      port: {port}
      targetPort: {port} # Default, should ideally match containerPort
"#,
        service_name = service_name,
        port = port
    )
}

pub fn generate_config_map(service_name: &str, _app_type: &str) -> String {
    format!(
        r#"apiVersion: v1
kind: ConfigMap
metadata:
  name: {service_name}-config
data:
  EXAMPLE_KEY: "example_value"
"#,
        service_name = service_name
    )
}
