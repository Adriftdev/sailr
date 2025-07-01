// src/templates/scaffolding.rs

#[derive(Debug, Clone)]
pub struct ServiceTemplate {
    pub deployment: String,
    pub service: String,
    pub config_map: String,
    pub ingress: Option<String>,
    pub hpa: Option<String>,
}

pub fn get_service_template(
    app_type: &str,
    service_name: &str,
    image: &str,
    port: u16,
) -> ServiceTemplate {
    match app_type {
        "web-app" => generate_web_app_template(service_name, image, port),
        "worker" => generate_worker_template(service_name, image),
        "database-client" => generate_database_client_template(service_name, image, port),
        "api" => generate_api_template(service_name, image, port),
        _ => generate_default_template(service_name, image, port),
    }
}

fn generate_web_app_template(service_name: &str, image: &str, port: u16) -> ServiceTemplate {
    let deployment = format!(
        r#"apiVersion: apps/v1
kind: Deployment
metadata:
  name: {service_name}
  labels:
    app: {service_name}
    type: web-app
spec:
  replicas: 3
  selector:
    matchLabels:
      app: {service_name}
  template:
    metadata:
      labels:
        app: {service_name}
        type: web-app
    spec:
      containers:
      - name: {service_name}
        image: {image}
        ports:
        - containerPort: {port}
        env:
        - name: PORT
          value: "{port}"
        - name: NODE_ENV
          valueFrom:
            configMapKeyRef:
              name: {service_name}-config
              key: NODE_ENV
        resources:
          requests:
            memory: "128Mi"
            cpu: "100m"
          limits:
            memory: "512Mi"
            cpu: "500m"
        livenessProbe:
          httpGet:
            path: /health
            port: {port}
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /ready
            port: {port}
          initialDelaySeconds: 5
          periodSeconds: 5
"#,
        service_name = service_name,
        image = image,
        port = port
    );

    let service = format!(
        r#"apiVersion: v1
kind: Service
metadata:
  name: {service_name}
  labels:
    app: {service_name}
    type: web-app
spec:
  selector:
    app: {service_name}
  ports:
    - protocol: TCP
      port: 80
      targetPort: {port}
  type: ClusterIP
"#,
        service_name = service_name,
        port = port
    );

    let config_map = format!(
        r#"apiVersion: v1
kind: ConfigMap
metadata:
  name: {service_name}-config
  labels:
    app: {service_name}
    type: web-app
data:
  NODE_ENV: "production"
  LOG_LEVEL: "info"
  PORT: "{port}"
"#,
        service_name = service_name,
        port = port
    );

    let ingress = Some(format!(
        r#"apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: {service_name}
  labels:
    app: {service_name}
    type: web-app
  annotations:
    nginx.ingress.kubernetes.io/rewrite-target: /
spec:
  rules:
  - host: {service_name}.local
    http:
      paths:
      - path: /
        pathType: Prefix
        backend:
          service:
            name: {service_name}
            port:
              number: 80
"#,
        service_name = service_name
    ));

    ServiceTemplate {
        deployment,
        service,
        config_map,
        ingress,
        hpa: None,
    }
}

fn generate_worker_template(service_name: &str, image: &str) -> ServiceTemplate {
    let deployment = format!(
        r#"apiVersion: apps/v1
kind: Deployment
metadata:
  name: {service_name}
  labels:
    app: {service_name}
    type: worker
spec:
  replicas: 2
  selector:
    matchLabels:
      app: {service_name}
  template:
    metadata:
      labels:
        app: {service_name}
        type: worker
    spec:
      containers:
      - name: {service_name}
        image: {image}
        env:
        - name: WORKER_TYPE
          valueFrom:
            configMapKeyRef:
              name: {service_name}-config
              key: WORKER_TYPE
        - name: QUEUE_URL
          valueFrom:
            secretKeyRef:
              name: {service_name}-secrets
              key: QUEUE_URL
        resources:
          requests:
            memory: "256Mi"
            cpu: "200m"
          limits:
            memory: "1Gi"
            cpu: "1000m"
"#,
        service_name = service_name,
        image = image
    );

    let config_map = format!(
        r#"apiVersion: v1
kind: ConfigMap
metadata:
  name: {service_name}-config
  labels:
    app: {service_name}
    type: worker
data:
  WORKER_TYPE: "background"
  LOG_LEVEL: "info"
  BATCH_SIZE: "10"
"#,
        service_name = service_name
    );

    ServiceTemplate {
        deployment,
        service: String::new(), // Workers typically don't need services
        config_map,
        ingress: None,
        hpa: None,
    }
}

fn generate_database_client_template(
    service_name: &str,
    image: &str,
    port: u16,
) -> ServiceTemplate {
    let deployment = format!(
        r#"apiVersion: apps/v1
kind: Deployment
metadata:
  name: {service_name}
  labels:
    app: {service_name}
    type: database-client
spec:
  replicas: 1
  selector:
    matchLabels:
      app: {service_name}
  template:
    metadata:
      labels:
        app: {service_name}
        type: database-client
    spec:
      containers:
      - name: {service_name}
        image: {image}
        ports:
        - containerPort: {port}
        env:
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: {service_name}-secrets
              key: DATABASE_URL
        - name: DB_HOST
          valueFrom:
            configMapKeyRef:
              name: {service_name}-config
              key: DB_HOST
        - name: DB_PORT
          valueFrom:
            configMapKeyRef:
              name: {service_name}-config
              key: DB_PORT
        resources:
          requests:
            memory: "128Mi"
            cpu: "100m"
          limits:
            memory: "512Mi"
            cpu: "500m"
"#,
        service_name = service_name,
        image = image,
        port = port
    );

    let service = format!(
        r#"apiVersion: v1
kind: Service
metadata:
  name: {service_name}
  labels:
    app: {service_name}
    type: database-client
spec:
  selector:
    app: {service_name}
  ports:
    - protocol: TCP
      port: {port}
      targetPort: {port}
  type: ClusterIP
"#,
        service_name = service_name,
        port = port
    );

    let config_map = format!(
        r#"apiVersion: v1
kind: ConfigMap
metadata:
  name: {service_name}-config
  labels:
    app: {service_name}
    type: database-client
data:
  DB_HOST: "localhost"
  DB_PORT: "5432"
  DB_NAME: "myapp"
  CONNECTION_POOL_SIZE: "10"
"#,
        service_name = service_name
    );

    ServiceTemplate {
        deployment,
        service,
        config_map,
        ingress: None,
        hpa: None,
    }
}

fn generate_api_template(service_name: &str, image: &str, port: u16) -> ServiceTemplate {
    let deployment = format!(
        r#"apiVersion: apps/v1
kind: Deployment
metadata:
  name: {service_name}
  labels:
    app: {service_name}
    type: api
spec:
  replicas: 3
  selector:
    matchLabels:
      app: {service_name}
  template:
    metadata:
      labels:
        app: {service_name}
        type: api
    spec:
      containers:
      - name: {service_name}
        image: {image}
        ports:
        - containerPort: {port}
        env:
        - name: PORT
          value: "{port}"
        - name: API_VERSION
          valueFrom:
            configMapKeyRef:
              name: {service_name}-config
              key: API_VERSION
        - name: JWT_SECRET
          valueFrom:
            secretKeyRef:
              name: {service_name}-secrets
              key: JWT_SECRET
        resources:
          requests:
            memory: "256Mi"
            cpu: "200m"
          limits:
            memory: "1Gi"
            cpu: "1000m"
        livenessProbe:
          httpGet:
            path: /api/health
            port: {port}
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /api/ready
            port: {port}
          initialDelaySeconds: 5
          periodSeconds: 5
"#,
        service_name = service_name,
        image = image,
        port = port
    );

    let service = format!(
        r#"apiVersion: v1
kind: Service
metadata:
  name: {service_name}
  labels:
    app: {service_name}
    type: api
spec:
  selector:
    app: {service_name}
  ports:
    - protocol: TCP
      port: 80
      targetPort: {port}
  type: ClusterIP
"#,
        service_name = service_name,
        port = port
    );

    let config_map = format!(
        r#"apiVersion: v1
kind: ConfigMap
metadata:
  name: {service_name}-config
  labels:
    app: {service_name}
    type: api
data:
  API_VERSION: "v1"
  LOG_LEVEL: "info"
  CORS_ORIGINS: "*"
  RATE_LIMIT: "100"
"#,
        service_name = service_name
    );

    let hpa = Some(format!(
        r#"apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: {service_name}
  labels:
    app: {service_name}
    type: api
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: {service_name}
  minReplicas: 2
  maxReplicas: 10
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 70
  - type: Resource
    resource:
      name: memory
      target:
        type: Utilization
        averageUtilization: 80
"#,
        service_name = service_name
    ));

    ServiceTemplate {
        deployment,
        service,
        config_map,
        ingress: None,
        hpa,
    }
}

fn generate_default_template(service_name: &str, image: &str, port: u16) -> ServiceTemplate {
    let deployment = generate_deployment(service_name, "default", image, 1, port);
    let service = generate_service(service_name, "default", port);
    let config_map = generate_config_map(service_name, "default");

    ServiceTemplate {
        deployment,
        service,
        config_map,
        ingress: None,
        hpa: None,
    }
}

// Legacy functions for backward compatibility
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
        - containerPort: {port}
        resources:
          requests:
            memory: "128Mi"
            cpu: "100m"
          limits:
            memory: "512Mi"
            cpu: "500m"
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
      targetPort: {port}
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

pub fn generate_secret_template(service_name: &str, app_type: &str) -> String {
    let secret_data = match app_type {
        "database-client" => {
            r#"  DATABASE_URL: "cG9zdGdyZXNxbDovL3VzZXI6cGFzc3dvcmRAbG9jYWxob3N0OjU0MzIvbXlhcHA=" # base64 encoded"#
        }
        "api" => r#"  JWT_SECRET: "bXlfc2VjcmV0X2p3dF9rZXk=" # base64 encoded"#,
        "worker" => {
            r#"  QUEUE_URL: "cmVkaXM6Ly9sb2NhbGhvc3Q6NjM3OS8wIyBiYXNlNjQgZW5jb2RlZA==" # base64 encoded"#
        }
        _ => r#"  SECRET_KEY: "bXlfc2VjcmV0X2tleQ==" # base64 encoded"#,
    };

    format!(
        r#"apiVersion: v1
kind: Secret
metadata:
  name: {service_name}-secrets
  labels:
    app: {service_name}
    type: {app_type}
type: Opaque
data:
{secret_data}
"#,
        service_name = service_name,
        app_type = app_type,
        secret_data = secret_data
    )
}
