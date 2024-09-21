use std::collections::HashSet;

use crate::spec::SubscriptionOptions;

#[derive(Debug, Clone)]
pub struct Subscription {
    pub subuid: i32,
    pub topics: HashSet<String>,
    pub options: Option<SubscriptionOptions>,
}
impl Subscription {
    pub fn get_options(&self) -> Option<&SubscriptionOptions> {
        self.options.as_ref()
    }
    pub fn cares_about(&self, topic: &String) -> bool {
        if let Some(options) = self.get_options() {
            if options.prefix.unwrap_or(false) {
                return self.topics
                    .iter()
                    .any(|topic_pat| topic.starts_with(topic_pat));
            }
        }
        self.topics.iter().any(|topic_name| *topic_name == *topic)
    }
}


