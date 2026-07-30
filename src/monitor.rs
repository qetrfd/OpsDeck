use crate::health::{HealthCheck, check_optional_url};
use crate::history::record_review;
use crate::intelligence::{Diagnosis, analyze_project_with_health};
use crate::{Project, ProjectStatus, project_status};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::thread;
use std::time::SystemTime;

#[derive(Debug, Clone)]
pub enum MonitorCommand {
    CheckProject(Project),
    CheckAll(Vec<Project>),
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum MonitorEvent {
    Started { project_name: String },
    Finished(MonitorResult),
}

#[derive(Debug, Clone)]
pub struct MonitorResult {
    pub project_name: String,
    pub status: Option<ProjectStatus>,
    pub health: HealthCheck,
    pub diagnosis: Option<Diagnosis>,
    pub error: Option<String>,
    pub history_error: Option<String>,
    pub checked_at: SystemTime,
}

pub struct MonitorHandle {
    command_sender: Sender<MonitorCommand>,
    event_receiver: Receiver<MonitorEvent>,
}

impl MonitorHandle {
    pub fn check_project(&self, project: Project) -> Result<(), String> {
        self.command_sender
            .send(MonitorCommand::CheckProject(project))
            .map_err(|error| format!("No se pudo solicitar la revisión: {error}"))
    }

    pub fn check_all(&self, projects: Vec<Project>) -> Result<(), String> {
        self.command_sender
            .send(MonitorCommand::CheckAll(projects))
            .map_err(|error| format!("No se pudo solicitar la revisión: {error}"))
    }

    pub fn try_recv(&self) -> Result<MonitorEvent, TryRecvError> {
        self.event_receiver.try_recv()
    }
}

impl Drop for MonitorHandle {
    fn drop(&mut self) {
        let _ = self.command_sender.send(MonitorCommand::Shutdown);
    }
}

pub fn spawn_monitor_worker() -> Result<MonitorHandle, String> {
    let (command_sender, command_receiver) = channel::<MonitorCommand>();
    let (event_sender, event_receiver) = channel::<MonitorEvent>();

    thread::Builder::new()
        .name("opsdeck-monitor".to_string())
        .spawn(move || {
            run_worker(command_receiver, event_sender);
        })
        .map_err(|error| format!("No se pudo iniciar el monitor: {error}"))?;

    Ok(MonitorHandle {
        command_sender,
        event_receiver,
    })
}

fn run_worker(command_receiver: Receiver<MonitorCommand>, event_sender: Sender<MonitorEvent>) {
    while let Ok(command) = command_receiver.recv() {
        match command {
            MonitorCommand::CheckProject(project) => {
                if !inspect_project(project, &event_sender) {
                    break;
                }
            }
            MonitorCommand::CheckAll(projects) => {
                for project in projects {
                    if !inspect_project(project, &event_sender) {
                        return;
                    }
                }
            }
            MonitorCommand::Shutdown => break,
        }
    }
}

fn inspect_project(project: Project, event_sender: &Sender<MonitorEvent>) -> bool {
    if event_sender
        .send(MonitorEvent::Started {
            project_name: project.name.clone(),
        })
        .is_err()
    {
        return false;
    }

    let project_name = project.name.clone();
    let target = project.path.to_string_lossy().to_string();
    let health = check_optional_url(project.health_url.as_deref());

    let result = match project_status(&target) {
        Ok(status) => {
            let diagnosis = analyze_project_with_health(&status, &health);

            let history_error = record_review(&project_name, &status, &health, &diagnosis).err();

            MonitorResult {
                project_name,
                status: Some(status),
                health,
                diagnosis: Some(diagnosis),
                error: None,
                history_error,
                checked_at: SystemTime::now(),
            }
        }
        Err(error) => MonitorResult {
            project_name,
            status: None,
            health,
            diagnosis: None,
            error: Some(error),
            history_error: None,
            checked_at: SystemTime::now(),
        },
    };

    event_sender.send(MonitorEvent::Finished(result)).is_ok()
}
