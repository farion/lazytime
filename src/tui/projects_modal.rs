#[derive(Debug, Clone)]
pub enum ProjectsModal {
    Project(ProjectModal),
    Rule(RuleModal),
    Confirm(ConfirmModal),
}

#[derive(Debug, Clone)]
pub enum ConfirmKind {
    DeleteProject { project_id: i64 },
    DeleteRule { rule_id: i64 },
}

#[derive(Debug, Clone)]
pub struct ConfirmModal {
    pub title: String,
    pub message: String,
    // 0=yes,1=no
    pub field_idx: usize,
    pub kind: ConfirmKind,
}

impl ConfirmModal {
    pub fn delete_project(project_id: i64) -> Self {
        Self {
            title: "Confirm delete".to_string(),
            message: "Delete selected project and all rules?".to_string(),
            field_idx: 1,
            kind: ConfirmKind::DeleteProject { project_id },
        }
    }

    pub fn delete_rule(rule_id: i64) -> Self {
        Self {
            title: "Confirm delete".to_string(),
            message: "Delete selected rule?".to_string(),
            field_idx: 1,
            kind: ConfirmKind::DeleteRule { rule_id },
        }
    }
}

#[derive(Debug, Clone)]
pub enum ProjectModalMode {
    Add,
    Edit,
}

#[derive(Debug, Clone)]
pub struct ProjectModal {
    pub mode: ProjectModalMode,
    pub name: String,
    pub sap: String,
    // 0=name,1=sap,2=ok,3=cancel
    pub field_idx: usize,
    pub editing_id: Option<i64>,
}

impl ProjectModal {
    pub fn new_add() -> Self {
        Self {
            mode: ProjectModalMode::Add,
            name: String::new(),
            sap: String::new(),
            field_idx: 0,
            editing_id: None,
        }
    }

    pub fn new_edit(id: i64, name: String, sap: String) -> Self {
        Self {
            mode: ProjectModalMode::Edit,
            name,
            sap,
            field_idx: 0,
            editing_id: Some(id),
        }
    }
}

#[derive(Debug, Clone)]
pub enum RuleModalMode {
    Add,
    Edit,
}

#[derive(Debug, Clone)]
pub struct RuleModal {
    pub mode: RuleModalMode,
    pub project_id: i64,
    pub app_id: String,
    pub name_regex: String,
    pub precedence: String,
    // 0=app_id,1=regex,2=precedence,3=ok,4=cancel
    pub field_idx: usize,
    pub editing_id: Option<i64>,
}

impl RuleModal {
    pub fn new_add(project_id: i64) -> Self {
        Self {
            mode: RuleModalMode::Add,
            project_id,
            app_id: String::new(),
            name_regex: String::new(),
            precedence: "0".to_string(),
            field_idx: 0,
            editing_id: None,
        }
    }

    pub fn new_edit(
        project_id: i64,
        rule_id: i64,
        app_id: String,
        name_regex: String,
        precedence: i64,
    ) -> Self {
        Self {
            mode: RuleModalMode::Edit,
            project_id,
            app_id,
            name_regex,
            precedence: precedence.to_string(),
            field_idx: 0,
            editing_id: Some(rule_id),
        }
    }
}
