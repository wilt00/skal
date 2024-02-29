#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::{Arc, Mutex};

use chrono::{DateTime, TimeZone, Timelike, Utc};
use futures::executor::block_on;
use windows::core::HRESULT;
use windows::{
    core::{Error, Result},
    Foundation::{EventRegistrationToken, TypedEventHandler},
    UI::Notifications::{
        KnownNotificationBindings,
        Management::{UserNotificationListener, UserNotificationListenerAccessStatus},
        UserNotificationChangedEventArgs, UserNotificationChangedKind,
    },
};
use winsafe::co::SS;
use winsafe::{co, gui, prelude::*, AnyResult, ExitThread, HWND};

const DEFAULT_HEIGHT: u32 = 150;
const DEFAULT_WIDTH: u32 = 300;

#[derive(Clone)]
struct MainWindow {
    wnd: gui::WindowMain,
    txt: gui::Label,
    token_ptr: Arc<Mutex<Option<TokenContainer>>>,
}

impl MainWindow {
    fn new() -> Self {
        let wnd = gui::WindowMain::new(gui::WindowMainOpts {
            title: "Skal".to_owned(),
            size: (DEFAULT_WIDTH, DEFAULT_HEIGHT),
            ..Default::default()
        });
        let txt = gui::Label::new(
            &wnd,
            gui::LabelOpts {
                text: "Waiting for notifications...".to_owned(),
                position: (10, 10),
                size: (DEFAULT_WIDTH - 20, DEFAULT_HEIGHT - 20),
                label_style: SS::LEFT,
                ..Default::default()
            },
        );
        let new_self = Self {
            wnd,
            txt,
            token_ptr: Arc::new(Mutex::new(None)),
        };
        new_self.events();
        new_self
    }

    pub fn run(&self) -> AnyResult<i32> {
        self.wnd.run_main(None)
    }

    fn events(&self) {
        let self_2 = self.clone();
        self.wnd.on().wm_create(move |_a| {
            match block_on(get_access()) {
                // Ok(()) => (),
                Ok(()) => {
                    HWND::NULL
                        .MessageBox(
                            "Ready to receive notifications",
                            "Access granted",
                            co::MB::ICONINFORMATION,
                        )
                        .unwrap();
                }
                Err(e) => error_dialog_and_quit(Box::new(e)),
            };
            match setup_listener() {
                Ok(token) => {
                    let mut token_ptr = self_2.token_ptr.lock().unwrap();
                    *token_ptr = Some(token);
                }
                Err(e) => error_dialog_and_quit(Box::new(e)),
            }
            Ok(0)
        });
    }

    // fn update_txt(&self, new_txt: String) {
    //     &self.txt.set_text(&new_txt);
    // }
}

struct TokenContainer {
    token: EventRegistrationToken,
}

impl Drop for TokenContainer {
    fn drop(&mut self) {
        UserNotificationListener::Current()
            .unwrap()
            .RemoveNotificationChanged(self.token)
            .unwrap()
    }
}

fn notification_handler(
    sender: &Option<UserNotificationListener>,
    args: &Option<UserNotificationChangedEventArgs>,
) -> Result<()> {
    let (listener, a) = match (sender, args) {
        (Some(listener), Some(a)) => (listener, a),
        _ => {
            println!("Error: one or more of expected handler params were missing");
            return Ok(());
        }
    };

    match a.ChangeKind() {
        Ok(UserNotificationChangedKind::Removed) => {
            println!("Warning: notification was removed");
            return Ok(());
        }
        Ok(UserNotificationChangedKind::Added) => (),
        _ => {
            println!("Error: unknown notification change kind");
            panic!()
        }
    }

    let notification = match listener.GetNotification(a.UserNotificationId()?) {
        Ok(n) => n,
        _ => {
            println!("Error: could not resolve notification");
            return Ok(());
        }
    };

    let app_display_info = notification.AppInfo()?.DisplayInfo()?;
    let app_name = app_display_info.DisplayName()?.to_string();
    // let logo_stream_ref = app_display_info.GetLogo(Size {
    //     Height: 64.0,
    //     Width: 64.0,
    // });

    let time: chrono::DateTime<Utc> = DateTime::from(
        Utc.timestamp_opt(notification.CreationTime()?.UniversalTime, 0)
            .unwrap(),
    );
    let (is_pm, hour) = time.hour12();

    let binding_type = KnownNotificationBindings::ToastGeneric()?;
    let text = notification
        .Notification()?
        .Visual()?
        .GetBinding(&binding_type)?
        .GetTextElements()?
        .into_iter()
        .map(|e| {
            e.Text()
                .and_then(|t| Ok(t.to_string()))
                .unwrap_or("\n".to_string())
        })
        .collect::<String>();

    println!(
        "{}, at {:02}:{:02} {}: {}",
        app_name,
        hour,
        time.minute(),
        if is_pm { "PM" } else { "AM" },
        text
    );

    Ok(())
}

async fn get_access() -> Result<()> {
    UserNotificationListener::Current()?
        .RequestAccessAsync()?
        .await
        .and_then(|status| match status {
            UserNotificationListenerAccessStatus::Allowed => Ok(()),
            _ => Err(Error::new(
                HRESULT::from_win32(0x80070005),
                "Notification access not granted",
            )),
        })
}

fn setup_listener() -> Result<TokenContainer> {
    let handler =
        TypedEventHandler::<UserNotificationListener, UserNotificationChangedEventArgs>::new(
            notification_handler,
        );

    let listener = UserNotificationListener::Current()?;

    println!("got listener");

    listener
        .NotificationChanged(&handler)
        .and_then(|token| Ok(TokenContainer { token }))
}

fn error_dialog_and_quit(e: Box<dyn std::error::Error>) {
    HWND::NULL
        .MessageBox(&e.to_string(), "Uncaught error", co::MB::ICONERROR)
        .unwrap();
    ExitThread(1);
}

fn main() -> Result<()> {
    match MainWindow::new().run() {
        Err(e) => error_dialog_and_quit(e),
        _ => (),
    }

    let _token = match setup_listener() {
        Ok(t) => t,
        Err(e) => {
            println!("{}", e);
            panic!();
        }
    };

    println!("Listener registered");

    Ok(())
}
