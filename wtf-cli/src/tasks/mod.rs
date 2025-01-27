pub mod github_tasks;
pub mod google_tasks;
pub mod jira_tasks;
pub mod worklog_tasks;

pub trait Task {
    async fn execute(&self) -> Result<(), Box<dyn std::error::Error>>;
}
