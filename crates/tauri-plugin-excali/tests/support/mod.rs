//! A mock Tauri app with the plugin, its ACL built from the permission
//! files and capability this crate ships, scripted dialogs and a scripted
//! network.

#![allow(dead_code)]

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::Value;
use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody, RuntimeAuthority};
use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY};
use tauri::utils::acl::capability::Capability;
use tauri::utils::acl::manifest::{Manifest, PermissionFile};
use tauri::utils::acl::resolved::Resolved;
use tauri::utils::platform::Target;
use tauri::webview::InvokeRequest;
use tauri::{App, AppHandle, Manager, WebviewWindow, WebviewWindowBuilder};

use tauri_plugin_excali::files::DialogSpec;
use tauri_plugin_excali::{Builder, FileDialogs, HttpResponse, Transport};

pub const CRATE: &str = env!("CARGO_MANIFEST_DIR");

/// Every `.toml` under `permissions/`.
pub fn permission_files() -> Vec<PermissionFile> {
    let mut paths = Vec::new();
    let mut dirs = vec![Path::new(CRATE).join("permissions")];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e == "toml") {
                paths.push(path);
            }
        }
    }
    paths.sort();
    paths
        .iter()
        .map(|p| toml::from_str(&std::fs::read_to_string(p).unwrap()).unwrap())
        .collect()
}

pub fn capability() -> Capability {
    let text = std::fs::read_to_string(Path::new(CRATE).join("capabilities/excali.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

/// What the scripted dialogs were asked, and answer.
#[derive(Clone, Default)]
pub struct Dialogs {
    pub asked: Arc<Mutex<Vec<(String, DialogSpec)>>>,
    pub answers: Arc<Mutex<VecDeque<Option<PathBuf>>>>,
}

impl Dialogs {
    pub fn answer(&self, path: Option<PathBuf>) {
        self.answers.lock().unwrap().push_back(path);
    }

    fn next(&self, which: &str, spec: &DialogSpec) -> Option<PathBuf> {
        self.asked
            .lock()
            .unwrap()
            .push((which.to_owned(), spec.clone()));
        self.answers
            .lock()
            .unwrap()
            .pop_front()
            .expect("an unexpected dialog")
    }
}

impl FileDialogs<MockRuntime> for Dialogs {
    fn pick_file(
        &self,
        _: &AppHandle<MockRuntime>,
        spec: &DialogSpec,
    ) -> tauri_plugin_excali::Result<Option<PathBuf>> {
        Ok(self.next("open", spec))
    }

    fn save_file(
        &self,
        _: &AppHandle<MockRuntime>,
        spec: &DialogSpec,
    ) -> tauri_plugin_excali::Result<Option<PathBuf>> {
        Ok(self.next("save", spec))
    }
}

/// Responses by URL; the URLs requested.
#[derive(Clone, Default)]
pub struct Network {
    pub responses: Arc<Mutex<BTreeMap<String, HttpResponse>>>,
    pub requested: Arc<Mutex<Vec<String>>>,
}

impl Network {
    pub fn serve(&self, url: &str, status: u16, location: Option<&str>, body: &str) {
        self.responses.lock().unwrap().insert(
            url.to_owned(),
            HttpResponse {
                status,
                location: location.map(str::to_owned),
                body: body.as_bytes().to_vec(),
            },
        );
    }
}

impl Transport for Network {
    fn get(&self, url: &str, _limit: u64) -> Result<HttpResponse, String> {
        self.requested.lock().unwrap().push(url.to_owned());
        self.responses
            .lock()
            .unwrap()
            .get(url)
            .cloned()
            .ok_or_else(|| format!("{url}: connection refused"))
    }
}

pub struct Harness {
    pub app: App<MockRuntime>,
    pub dialogs: Dialogs,
    pub network: Network,
}

impl Harness {
    pub fn new() -> Harness {
        Harness::with(|b| b)
    }

    pub fn with(configure: impl FnOnce(Builder<MockRuntime>) -> Builder<MockRuntime>) -> Harness {
        let dialogs = Dialogs::default();
        let network = Network::default();
        let mut acl = BTreeMap::new();
        acl.insert("excali".to_owned(), Manifest::new(permission_files(), None));
        let capability = capability();
        let mut capabilities = BTreeMap::new();
        capabilities.insert(capability.identifier.clone(), capability);
        let resolved = Resolved::resolve(&acl, capabilities, Target::current()).unwrap();
        let mut context = mock_context(noop_assets());
        *context.runtime_authority_mut() = RuntimeAuthority::new(acl, resolved);
        let plugin = configure(
            Builder::new()
                .dialogs(dialogs.clone())
                .library_transport(network.clone()),
        )
        .build();
        let app = mock_builder().plugin(plugin).build(context).unwrap();
        Harness {
            app,
            dialogs,
            network,
        }
    }

    /// The window `label`, opened on first use.
    pub fn window(&self, label: &str) -> WebviewWindow<MockRuntime> {
        self.app.get_webview_window(label).unwrap_or_else(|| {
            WebviewWindowBuilder::new(&self.app, label, Default::default())
                .build()
                .unwrap()
        })
    }

    /// `invoke("plugin:excali|<command>", args)` from the window `label`.
    pub fn invoke_in(
        &self,
        label: &str,
        command: &str,
        args: Value,
    ) -> Result<InvokeResponseBody, Value> {
        let window = self.window(label);
        tauri::test::get_ipc_response(
            &window,
            InvokeRequest {
                cmd: format!("plugin:excali|{command}"),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url: "tauri://localhost".parse().unwrap(),
                body: InvokeBody::Json(args),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_owned(),
            },
        )
    }

    pub fn invoke(&self, command: &str, args: Value) -> Result<InvokeResponseBody, Value> {
        self.invoke_in("main", command, args)
    }

    /// A JSON answer, or the rejection's message.
    pub fn call(&self, command: &str, args: Value) -> Result<Value, String> {
        match self.invoke(command, args) {
            Ok(body) => Ok(json(body)),
            Err(Value::String(message)) => Err(message),
            Err(other) => panic!("a rejection that is not a message: {other}"),
        }
    }
}

pub fn json(body: InvokeResponseBody) -> Value {
    match body {
        InvokeResponseBody::Json(text) => serde_json::from_str(&text).unwrap(),
        InvokeResponseBody::Raw(bytes) => panic!("raw bytes ({} bytes), not JSON", bytes.len()),
    }
}

/// A scene with a rectangle and a text.
pub const SCENE: &str = r##"{
  "type": "excalidraw",
  "version": 2,
  "source": "https://excalidraw.com",
  "elements": [
    { "id": "rect", "type": "rectangle", "x": 0, "y": 0, "width": 120, "height": 60,
      "strokeColor": "#1e1e1e", "backgroundColor": "#a5d8ff", "fillStyle": "solid",
      "seed": 1, "version": 1, "versionNonce": 1 },
    { "id": "label", "type": "text", "x": 10, "y": 80, "width": 50, "height": 25,
      "text": "Hello", "originalText": "Hello", "fontSize": 20, "fontFamily": 5,
      "seed": 2, "version": 1, "versionNonce": 2 }
  ],
  "appState": { "viewBackgroundColor": "#ffffff" },
  "files": {}
}"##;

/// A library with one item.
pub const LIBRARY: &str = r##"{
  "type": "excalidrawlib",
  "version": 2,
  "source": "https://excalidraw.com",
  "libraryItems": [
    { "id": "item-1", "status": "published", "created": 1,
      "elements": [
        { "id": "e1", "type": "ellipse", "x": 0, "y": 0, "width": 40, "height": 40,
          "seed": 3, "version": 1, "versionNonce": 3 }
      ] }
  ]
}"##;

/// A fresh directory under the target directory.
pub fn temp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("tauri-plugin-excali")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
