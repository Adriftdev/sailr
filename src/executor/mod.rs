#[derive(Clone)]
pub struct Server {
    pub ip: String,
    pub user: String,
    pub key: String,
}

#[derive(PartialEq)]
pub enum ServerType {
    Local,
    Remote,
    Minikube,
}

pub struct Command {
    pub command: String,
    pub sudo: bool,
}

impl Command {
    pub fn new(command: String, sudo: bool) -> Command {
        Command { command, sudo }
    }

    pub fn update_command(&mut self, command: String) {
        self.command = command;
    }
}

pub struct Executor {
    pub env_type: ServerType,
    pub user: Option<String>,
    pub server: Option<Server>,
    ssh_key: Option<String>,
}

impl Executor {
    pub fn new() -> Executor {
        Executor {
            env_type: ServerType::Local,
            ssh_key: None,
            user: None,
            server: None,
        }
    }

    pub fn run(&self, mut command: Command) {
        match self.env_type {
            ServerType::Local => {
                let interpeter = if command.sudo { "sudo sh" } else { "sh" };
                let handle = std::process::Command::new(interpeter)
                    .arg("-c")
                    .arg(command.command)
                    .spawn()
                    .expect("failed to execute process");

                let res = handle.wait_with_output().unwrap();

                if res.status.success() {
                    println!("Command executed successfully");
                } else {
                    println!("Command failed");
                }
            }
            ServerType::Minikube => {
                if command.sudo {
                    command.update_command(format!("sudo {}", &command.command));
                }
                let handle = std::process::Command::new("minikube")
                    .arg("ssh")
                    .arg(command.command)
                    .spawn()
                    .expect("failed to execute process");

                let res = handle.wait_with_output().unwrap();

                if res.status.success() {
                    println!("Command executed successfully");
                } else {
                    println!("Command failed");
                }
            }
            ServerType::Remote => {
                if command.sudo {
                    command.update_command(format!("sudo {}", &command.command));
                }
                let ssh_command = format!(
                    "ssh -i {:?} {:?}@{:?} '{:?}'",
                    self.ssh_key,
                    self.user,
                    self.server.as_ref().unwrap().ip,
                    command.command
                );

                let handle = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(&ssh_command)
                    .spawn()
                    .expect("failed to execute process");

                let res = handle.wait_with_output().unwrap();

                if res.status.success() {
                    println!("Command executed successfully");
                } else {
                    println!("Command failed");
                }
            }
        }
    }
}
