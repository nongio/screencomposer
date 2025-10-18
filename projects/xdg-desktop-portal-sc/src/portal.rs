use std::collections::HashMap;
use std::convert::TryFrom;
use std::sync::Arc;

use nix::unistd::pipe;
use tokio::sync::Mutex;
use tracing::{info, warn};
use zbus::dbus_interface;
use zbus::fdo;
use zbus::object_server::ObjectServer;
use zbus::zvariant::{
    Array, Dict, ObjectPath, OwnedFd, OwnedObjectPath, OwnedValue, Signature, StructureBuilder,
    Value,
};
use zbus::{Connection, SignalContext};

const DESKTOP_PATH: &str = "/org/freedesktop/portal/desktop";
const REQUEST_PREFIX: &str = "/org/freedesktop/portal/desktop/request";
const SESSION_PREFIX: &str = "/org/freedesktop/portal/desktop/session";

#[derive(Default)]
struct PortalState {
    next_session: u64,
    next_request: u64,
}

#[derive(Clone)]
pub struct ScreenCastPortal {
    connection: Connection,
    state: Arc<Mutex<PortalState>>,
}

impl ScreenCastPortal {
    pub fn new(connection: Connection) -> Self {
        Self {
            connection,
            state: Arc::new(Mutex::new(PortalState::default())),
        }
    }

    async fn next_request_path(&self, token: Option<String>) -> zbus::Result<OwnedObjectPath> {
        let mut state = self.state.lock().await;
        let name = token.unwrap_or_else(|| {
            state.next_request += 1;
            format!("req{}", state.next_request)
        });
        let path = format!("{}/{}", REQUEST_PREFIX, name);
        OwnedObjectPath::try_from(path).map_err(Into::into)
    }

    async fn next_session_path(&self, token: Option<String>) -> zbus::Result<OwnedObjectPath> {
        let mut state = self.state.lock().await;
        let name = token.unwrap_or_else(|| {
            state.next_session += 1;
            format!("session{}", state.next_session)
        });
        let path = format!("{}/{}", SESSION_PREFIX, name);
        OwnedObjectPath::try_from(path).map_err(Into::into)
    }

    async fn register_request(
        &self,
        object_server: &ObjectServer,
        path: &OwnedObjectPath,
    ) -> zbus::Result<()> {
        object_server.at(path.clone(), Request::new()).await?;
        Ok(())
    }

    async fn emit_response(
        &self,
        object_server: &ObjectServer,
        path: &OwnedObjectPath,
        response: u32,
        results: HashMap<String, OwnedValue>,
    ) -> fdo::Result<()> {
        info!(request = %path.as_str(), response, "Emitting Request::Response");
        let signal_ctx = SignalContext::new(&self.connection, path.clone())?.into_owned();
        let value_results: HashMap<_, _> = results
            .into_iter()
            .map(|(key, value)| (key, Value::from(value)))
            .collect();
        Request::response(&signal_ctx, response, &value_results).await?;
        object_server.remove::<Request, _>(path).await?;
        Ok(())
    }

    fn extract_token(options: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
        options.get(key).and_then(|value| {
            if let Ok(s) = <&str>::try_from(value) {
                Some(s.to_string())
            } else {
                value
                    .try_clone()
                    .ok()
                    .and_then(|owned| String::try_from(owned).ok())
            }
        })
    }

    fn build_streams_value() -> zbus::Result<OwnedValue> {
        let mut dict = Dict::new(Signature::try_from("s")?, Signature::try_from("v")?);
        dict.append(
            Value::from("source_type"),
            Value::from(OwnedValue::from(1u32)),
        )?;

        let structure = StructureBuilder::new()
            .add_field(777u32)
            .append_field(Value::from(dict))
            .build();

        let mut array = Array::new(Signature::try_from("(ua{sv})")?);
        array.append(Value::from(structure))?;

        OwnedValue::try_from(Value::from(array)).map_err(Into::into)
    }
}

#[dbus_interface(name = "org.freedesktop.portal.ScreenCast")]
impl ScreenCastPortal {
    async fn create_session(
        &self,
        options: HashMap<String, OwnedValue>,
        #[zbus(object_server)] object_server: &ObjectServer,
    ) -> fdo::Result<OwnedObjectPath> {
        info!(?options, "CreateSession called");
        let request_token = Self::extract_token(&options, "handle_token");
        let session_token = Self::extract_token(&options, "session_handle_token");

        let session_path = self.next_session_path(session_token).await?;
        let request_path = self.next_request_path(request_token).await?;

        info!(session = %session_path, "Registering session object");
        object_server
            .at(session_path.clone(), Session::new(session_path.clone()))
            .await?;

        self.register_request(object_server, &request_path).await?;

        let mut results: HashMap<String, OwnedValue> = HashMap::new();
        results.insert(
            "session_handle".to_string(),
            OwnedValue::from(ObjectPath::from(&session_path)),
        );

        self.emit_response(object_server, &request_path, 0, results)
            .await
            .map_err(|err| fdo::Error::Failed(err.to_string()))?;

        Ok(request_path)
    }

    async fn select_sources(
        &self,
        session: OwnedObjectPath,
        options: HashMap<String, OwnedValue>,
        #[zbus(object_server)] object_server: &ObjectServer,
    ) -> fdo::Result<OwnedObjectPath> {
        info!(session = %session, ?options, "SelectSources called");
        let request_path = self
            .next_request_path(Self::extract_token(&options, "handle_token"))
            .await?;
        self.register_request(object_server, &request_path).await?;
        let results = HashMap::new();
        self.emit_response(object_server, &request_path, 0, results)
            .await
            .map_err(|err| fdo::Error::Failed(err.to_string()))?;
        Ok(request_path)
    }

    async fn start(
        &self,
        session: OwnedObjectPath,
        parent_window: &str,
        options: HashMap<String, OwnedValue>,
        #[zbus(object_server)] object_server: &ObjectServer,
    ) -> fdo::Result<OwnedObjectPath> {
        info!(session = %session, parent_window, ?options, "Start called");
        let request_path = self
            .next_request_path(Self::extract_token(&options, "handle_token"))
            .await?;
        self.register_request(object_server, &request_path).await?;

        let mut results = HashMap::new();
        let streams_value =
            Self::build_streams_value().map_err(|err| fdo::Error::Failed(err.to_string()))?;
        results.insert("streams".to_string(), streams_value);

        self.emit_response(object_server, &request_path, 0, results)
            .await
            .map_err(|err| fdo::Error::Failed(err.to_string()))?;
        Ok(request_path)
    }

    async fn open_pipe_wire_remote(
        &self,
        session: OwnedObjectPath,
        options: HashMap<String, OwnedValue>,
        #[zbus(object_server)] object_server: &ObjectServer,
    ) -> fdo::Result<OwnedFd> {
        info!(session = %session, ?options, "OpenPipeWireRemote called");

        if object_server
            .interface::<_, Session>(&session)
            .await
            .is_err()
        {
            warn!(session = %session, "OpenPipeWireRemote called with unknown session");
            return Err(fdo::Error::InvalidArgs("Unknown session".to_string()).into());
        }

        let (read_end, write_end) = pipe().map_err(|err| fdo::Error::Failed(err.to_string()))?;
        // Close the read end immediately; the write end is returned to the caller.
        drop(read_end);
        Ok(OwnedFd::from(write_end))
    }
}

#[derive(Clone, Default)]
struct Request;

impl Request {
    fn new() -> Self {
        Self
    }
}

#[dbus_interface(name = "org.freedesktop.portal.Request")]
impl Request {
    #[dbus_interface(signal)]
    async fn response(
        ctx: &SignalContext<'_>,
        response: u32,
        results: &HashMap<String, Value<'_>>,
    ) -> zbus::Result<()>;
}

#[derive(Clone)]
struct Session {
    path: OwnedObjectPath,
}

impl Session {
    fn new(path: OwnedObjectPath) -> Self {
        Self { path }
    }
}

#[dbus_interface(name = "org.freedesktop.portal.Session")]
impl Session {
    async fn close(
        &self,
        #[zbus(object_server)] object_server: &ObjectServer,
        #[zbus(signal_context)] ctx: SignalContext<'_>,
    ) -> fdo::Result<()> {
        info!(session = %self.path, "Session.Close called");
        info!(session = %self.path, "Session::Closed signal emitted");
        Session::closed(&ctx)
            .await
            .map_err(|err| fdo::Error::Failed(err.to_string()))?;
        object_server
            .remove::<Session, _>(&self.path)
            .await
            .map_err(|err| fdo::Error::Failed(err.to_string()))?;
        Ok(())
    }

    #[dbus_interface(signal)]
    async fn closed(ctx: &SignalContext<'_>) -> zbus::Result<()>;
}

pub fn desktop_path() -> &'static str {
    DESKTOP_PATH
}
