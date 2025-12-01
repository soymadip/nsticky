use anyhow::Result;

// 定义请求和响应类型
#[derive(Debug)]
pub enum Request {
    Add { window_id: u64 },
    Remove { window_id: u64 },
    List,
    ToggleActive,
    Stage(StageArgs),
    Unstage(UnstageArgs),
}

#[derive(Debug)]
pub struct StageArgs {
    pub window_id: Option<u64>,
    pub all: bool,
    pub list: bool,
    pub active: bool,
}

#[derive(Debug)]
pub struct UnstageArgs {
    pub window_id: Option<u64>,
    pub all: bool,
    pub active: bool,
}

#[derive(Debug)]
pub enum Response {
    Success(String),
    Error(String),
    Data(String),
}

// 将字符串命令解析为Request
pub fn parse_request(line: &str) -> Result<Request> {
    let line = line.trim();
    let mut parts = line.split_whitespace();

    match parts.next() {
        Some("add") => {
            if let Some(id_str) = parts.next() {
                if let Ok(id) = id_str.parse::<u64>() {
                    Ok(Request::Add { window_id: id })
                } else {
                    Err(anyhow::anyhow!("Invalid window id"))
                }
            } else {
                Err(anyhow::anyhow!("Missing window id"))
            }
        }
        Some("remove") => {
            if let Some(id_str) = parts.next() {
                if let Ok(id) = id_str.parse::<u64>() {
                    Ok(Request::Remove { window_id: id })
                } else {
                    Err(anyhow::anyhow!("Invalid window id"))
                }
            } else {
                Err(anyhow::anyhow!("Missing window id"))
            }
        }
        Some("list") => Ok(Request::List),
        Some("toggle_active") => Ok(Request::ToggleActive),
        Some("stage") => {
            let arg = parts.next();
            match arg {
                Some("--all") => Ok(Request::Stage(StageArgs {
                    window_id: None,
                    all: true,
                    list: false,
                    active: false,
                })),
                Some("--list") => Ok(Request::Stage(StageArgs {
                    window_id: None,
                    all: false,
                    list: true,
                    active: false,
                })),
                Some("--active") => Ok(Request::Stage(StageArgs {
                    window_id: None,
                    all: false,
                    list: false,
                    active: true,
                })),
                Some(id_str) => {
                    if let Ok(id) = id_str.parse::<u64>() {
                        Ok(Request::Stage(StageArgs {
                            window_id: Some(id),
                            all: false,
                            list: false,
                            active: false,
                        }))
                    } else {
                        Err(anyhow::anyhow!("Invalid window id"))
                    }
                }
                None => Err(anyhow::anyhow!("Missing argument for stage")),
            }
        }
        Some("unstage") => {
            let arg = parts.next();
            match arg {
                Some("--all") => Ok(Request::Unstage(UnstageArgs {
                    window_id: None,
                    all: true,
                    active: false,
                })),
                Some("--active") => Ok(Request::Unstage(UnstageArgs {
                    window_id: None,
                    all: false,
                    active: true,
                })),
                Some(id_str) => {
                    if let Ok(id) = id_str.parse::<u64>() {
                        Ok(Request::Unstage(UnstageArgs {
                            window_id: Some(id),
                            all: false,
                            active: false,
                        }))
                    } else {
                        Err(anyhow::anyhow!("Invalid window id"))
                    }
                }
                None => Err(anyhow::anyhow!("Missing argument for unstage")),
            }
        }
        _ => Err(anyhow::anyhow!("Unknown command")),
    }
}

// 将Response转换为字符串
pub fn format_response(response: Response) -> String {
    match response {
        Response::Success(msg) => msg,
        Response::Error(msg) => format!("Error: {msg}"),
        Response::Data(data) => data,
    }
}
