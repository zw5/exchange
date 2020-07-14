use std::env;
use std::path;
use std::process;
use std::fs;
use std::string;
use std::io;
use std::str::Split;

use std::io::{Write};
use std::process::{Command, Stdio};

use serde::{Serialize, Deserialize};

macro_rules! hashmap {
    (@single $($x:tt)*) => (());
    (@count $($rest:expr),*) => (<[()]>::len(&[$(hashmap!(@single $rest)),*]));

    ($($key:expr => $value:expr,)+) => { hashmap!($($key => $value),+) };
    ($($key:expr => $value:expr),*) => {
        {
            let _cap = hashmap!(@count $($key),*);
            let mut _map = ::std::collections::HashMap::with_capacity(_cap);
            $(
                _map.insert($key, $value);
            )*
            _map
        }
    };
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceAuth {
    device_id: String,
    account_id: String,
    secret: String,
    created: std::collections::HashMap<String, String>,
}

struct AuthFromFile {
    account_id: String,
    device_id: String,
    secret: String,
}

impl AuthFromFile {
    fn from_string(mut data: Split<&str>) -> Self {
        Self {
            account_id: data.nth(0).unwrap().to_string(),
            device_id: data.nth(0).unwrap().to_string(),
            secret: data.nth(0).unwrap().to_string(),
        }
    }
    fn from_device_auth(data: DeviceAuth) -> Self {
        Self {
            account_id: data.account_id,
            device_id: data.device_id,
            secret: data.secret
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AuthDetails {
    pub access_token: String,
    expires_in: i64,
    expires_at: String,
    token_type: String,
    refresh_token: String,
    refresh_expires_at: String,
    pub account_id: String,
    client_id: String,
    internal_client: bool,
    client_service: String,
    app: String,
    in_app_id: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeCode {
    code: String,
    expires_in_seconds: i32,
    creating_client_id: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let key = "PATH";
    let path = env::var(key).unwrap();
    let ends_with_comma = path.ends_with(";");
    let current_file = env::current_exe().unwrap().to_str().unwrap().to_string();
    let save_file_path = path::Path::new(&env::var("AppData").unwrap()).join("exchange").join("data.ini");
    let metadata = fs::metadata(save_file_path).unwrap();
    if metadata.len() == 0 {
        let mut template = path;
        let ending = if ends_with_comma { "".to_string() } else { ";".to_string() };
        template.push_str(&ending);
        template.push_str(&current_file);
        println!("Program not in path, adding...");
        let path_add = format!("{{[Environment]::SetEnvironmentVariable(\"PATH\", \"{}\", \"User\")}}", template);
        run_raw(&path_add, false).unwrap();
        let client = reqwest::Client::new();
        let mut info = String::new();
        process::Command::new("https://www.epicgames.com/id/api/redirect?clientId=3446cd72694c4a4485d81b77adbb2141&responseType=code");
        println!("Please input the auth code.");
        io::stdin()
            .read_line(&mut info)
            .expect("Failed to read line.");
        trim_newline(&mut info);
        let url = "https://account-public-service-prod.ol.epicgames.com/account/api/oauth/token";
        let form = hashmap!{
            "grant_type" => "authorization_code",
            "code" => &info,
        };
        let client_id = "3446cd72694c4a4485d81b77adbb2141";
        let client_secret = "9209d4a5e25a457fb9b07489d313b41a";
        let res: AuthDetails = client.post(url)
            .form(&form)
            .basic_auth(client_id, Some(client_secret))
            .send()
            .await?
            .json()
            .await?;
        let url = format!("https://account-public-service-prod.ol.epicgames.com/account/api/public/account/{}/deviceAuth", res.account_id);
        let json: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let res: DeviceAuth = client.post(&url)
            .json(&json)
            .bearer_auth(res.access_token)
            .send()
            .await?
            .json()
            .await?;
        write_application_data(AuthFromFile::from_device_auth(res))
    }
    let data = get_application_data();
    let client = reqwest::Client::new();

    let auth_details = ios_authenticate(&client, data).await?;
    let exchange_code: ExchangeCode = client.get("https://account-public-service-prod.ol.epicgames.com/account/api/oauth/exchange")
        .bearer_auth(auth_details.access_token)
        .send()
        .await?
        .json()
        .await?;
    let option = env::args().nth(1);
    match option {
        Some(code) =>
            match &code[..] {
                "-c" => set_exchange_code_in_clipboard(exchange_code.code).await?,
                "-b" => authenticate_by_exchange_code(&exchange_code.code).await?,
                "-a" => todo!(),
                _ => unimplemented!(),
            },
        None => set_exchange_code_in_clipboard(exchange_code.code).await?,
    }
    Ok(())
}

fn run_raw(script: &str, print_commands: bool) -> Result<(), reqwest::Error> {
    let mut cmd = Command::new("PowerShell");
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let mut process = cmd.args(&["-Command", "-"]).spawn().unwrap();
    let stdin = process.stdin.as_mut().unwrap();

    for line in script.lines() {
        if print_commands {
            println!("{}", line)
        };
        writeln!(stdin, "{}", line).unwrap();
    }

    process.wait_with_output().unwrap();

    Ok(())
}

async fn set_exchange_code_in_clipboard(code: String) -> Result<(), reqwest::Error>{
    let command = format!("Set-Clipboard -Value \"{}\"", code);
    run_raw(&command, false).expect("Couldn't set clipboard");
    Ok(())
}

async fn authenticate_by_exchange_code(code: &str) -> Result<(), reqwest::Error> {
    let client = reqwest::Client::new();
    let payload = hashmap!{
        "grant_type" => "exchange_code",
        "code" => code
    };
    let url = "https://account-public-service-prod.ol.epicgames.com/account/api/oauth/token";
    let client_id = "ec684b8c687f479fadea3cb2ad83f5c6";
    let client_secret = "e1f31c211f28413186262d37a13fc84d";
    client.post(url)
        .form(&payload)
        .basic_auth(client_id, Some(client_secret))
        .send()
        .await?
        .json()
        .await?;

    Ok(())
}

fn trim_newline(s: &mut String) {
    if s.ends_with('\n') {
        s.pop();
        if s.ends_with('\r') {
            s.pop();
        }
    }
}

fn path_in_path(paths: String, current_file: String) -> bool {
    let paths = paths.split(";");
    let _ = for path_route in paths {
        let path = path::Path::new(path_route).to_str().unwrap().to_string();
        if path == current_file {
            return true;
        }
    };
    false
}

async fn ios_authenticate(client: &reqwest::Client, data: AuthFromFile) -> Result<AuthDetails, reqwest::Error> {
    let payload = hashmap!{
        "grant_type" => "device_auth",
        "device_id" => &data.device_id,
        "account_id" => &data.account_id,
        "secret" => &data.secret,
        "token_type" => "eg1",
    };
    let url = "https://account-public-service-prod.ol.epicgames.com/account/api/oauth/token";
    let client_id = "3446cd72694c4a4485d81b77adbb2141";
    let client_secret = "9209d4a5e25a457fb9b07489d313b41a";
    let res: AuthDetails = client.post(url)
        .basic_auth(client_id, Some(client_secret))
        .form(&payload)
        .send()
        .await?
        .json()
        .await?;
    Ok(res)
}

fn get_application_data() -> AuthFromFile {
    let save_file = path::Path::new(&env::var("AppData").unwrap()).join("exchange").join("data.ini");
    let data = exist_or_create(save_file);
    AuthFromFile::from_string(data.split(";"))
}

fn write_application_data(data: AuthFromFile) {
    let save_file = path::Path::new(&env::var("AppData").unwrap()).join("exchange").join("data.ini");
    let data = format!("{};{};{}", data.account_id, data.device_id, data.secret);
    write_to_file(save_file, data)
}

fn exist_or_create(path: path::PathBuf) -> string::String {
    let save_data = fs::read_to_string(path.clone());
    match save_data {
        Ok(exists) =>  {
            exists
        },
        Err(_) => create_file(path)
    }
}

fn create_file(path: path::PathBuf) -> String {
    println!("File not found, creating file in {:?}", &path);
    fs::create_dir(path.parent().unwrap()).unwrap();
    fs::write(path, "").expect("No permission to write files.");
    "".into()
}

fn write_to_file(path: path::PathBuf, text: String) {
    fs::write(path, text).expect("Couldn't write setting");
}