use crate::User;
use ahash::AHashMap;
use std::collections::HashMap;
pub struct Scores {
    ids: AHashMap<u64, User>,
    names: HashMap<String, u64>,
}

impl Scores {
    pub fn insert(&mut self, user: User) {
        self.names
            .insert(format!("{}#{}", user.username, user.discriminator), user.id);
        self.ids.insert(user.id, user);
    }
    #[must_use]
    pub fn get(&self, identifier: &str) -> Option<&User> {
        if let Ok(id) = identifier.parse::<u64>() {
            return self.ids.get(&id);
        }
        if let Some(id) = self.names.get(identifier) {
            return self.ids.get(id);
        }
        None
    }
    pub fn new(users: Vec<User>) -> Self {
        Self {
            names: users
                .iter()
                .map(|v| (format!("{}#{}", v.username, v.discriminator), v.id))
                .collect(),
            ids: users.into_iter().map(|v| (v.id, v)).collect(),
        }
    }
}
