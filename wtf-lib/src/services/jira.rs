pub struct JiraService {
    client: Client,
    base_url: String,
    api_token: String,
}

impl JiraService {
    pub fn new(base_url: String, api_token: String) -> Self {
        JiraService {
            client: Client::new(),
            base_url,
            api_token,
        }
    }

    pub async fn get_issue(&self, issue_id: &str) -> Result<JiraIssue, JiraError> {
        let url = format!("{}/rest/api/3/issue/{}", self.base_url, issue_id);
        let response = self.client
        .get(&url)
        .bearer_auth(&self.api_token)
        .send()
        .await?;

        if response.status().is_success() {
            let issue: JiraIssue = response.json().await?;
            Ok(issue)
        } else {
            Err(JiraError::ApiError(response.status().to_string()));
        }
    }
}