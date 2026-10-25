//! The notification centre of macOS: `UNUserNotificationCenter`.
//!
//! The centre works only for a process that runs from an app bundle. A binary that `cargo run`
//! starts has no bundle identifier, and a call to the centre then raises an exception that ends
//! the process. So [`Platform::start`] looks for the bundle first, and a run without one has no
//! notifications.
//!
//! The delegate is the one class of this crate that needs `unsafe`: the macro of `objc2` that
//! declares an Objective-C class expands to it.

use std::sync::Arc;

use block2::RcBlock;
use nh_core::host::{HostNotification, NotificationAction};
use nh_proto::api::common::Urgency;
use objc2::{
    AnyThread, DefinedClass, define_class, msg_send,
    rc::Retained,
    runtime::{Bool, NSObject, NSObjectProtocol, ProtocolObject},
};
use objc2_foundation::{NSArray, NSBundle, NSError, NSSet, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotification, UNNotificationAction,
    UNNotificationActionOptions, UNNotificationCategory, UNNotificationCategoryOptions,
    UNNotificationInterruptionLevel, UNNotificationPresentationOptions, UNNotificationRequest,
    UNNotificationResponse, UNNotificationSound, UNUserNotificationCenter,
    UNUserNotificationCenterDelegate,
};

use super::{Click, Permission, Response};

/// The category of an approval: it has the buttons.
const APPROVAL: &str = "nh.approval";
/// The category of an approval that the banner does not show enough to allow: it has Deny only.
const DENY_ONLY: &str = "nh.deny-only";
/// The category of the other notifications: a click opens the card.
const PLAIN: &str = "nh.open";
const ALLOW: &str = "nh.allow";
const DENY: &str = "nh.deny";
/// The identifier that the system gives to a click on the body of a banner.
const DEFAULT_ACTION: &str = "com.apple.UNNotificationDefaultActionIdentifier";

struct Ivars {
    on_response: Arc<dyn Fn(Response) + Send + Sync>,
}

define_class!(
    // SAFETY: `NSObject` has no requirements for its subclasses, the class has no `Drop`, and the
    // ivars are `Send` and `Sync`.
    #[unsafe(super(NSObject))]
    #[name = "NeoharNotificationDelegate"]
    #[ivars = Ivars]
    struct Delegate;

    // SAFETY: `NSObjectProtocol` has no required methods that this class would break.
    unsafe impl NSObjectProtocol for Delegate {}

    // SAFETY: the signatures are those of the protocol.
    unsafe impl UNUserNotificationCenterDelegate for Delegate {
        // A banner shows also when the app is in front. The core posts a notification only for a
        // pane without the focus of the person.
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            completion: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            completion.call((UNNotificationPresentationOptions::Banner
                | UNNotificationPresentationOptions::List
                | UNNotificationPresentationOptions::Sound,));
        }

        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            completion: &block2::DynBlock<dyn Fn()>,
        ) {
            let identifier = response.notification().request().identifier().to_string();
            let click = match response.actionIdentifier().to_string().as_str() {
                ALLOW => Some(Click::Allow),
                DENY => Some(Click::Deny),
                DEFAULT_ACTION => Some(Click::Open),
                // A dismissed banner asks for nothing.
                _ => None,
            };
            if let Some(click) = click {
                (self.ivars().on_response)(Response { click, identifier });
            }
            completion.call(());
        }
    }
);

impl Delegate {
    fn new(on_response: Arc<dyn Fn(Response) + Send + Sync>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(Ivars { on_response });
        // SAFETY: `NSObject` has an `init` that takes nothing.
        unsafe { msg_send![super(this), init] }
    }
}

/// The centre, and the delegate that must stay alive as long as the centre uses it (the centre
/// holds its delegate weakly).
struct Live {
    center: Retained<UNUserNotificationCenter>,
    _delegate: Retained<Delegate>,
}

// SAFETY: Apple documents `UNUserNotificationCenter` as safe to use from any thread. The delegate
// holds an `Arc<dyn Fn + Send + Sync>` and nothing else, and nothing else touches it.
unsafe impl Send for Live {}
// SAFETY: see above.
unsafe impl Sync for Live {}

/// The category for the buttons that the core asks for: Allow and Deny, Deny only, or none.
fn category(actions: &[NotificationAction]) -> &'static str {
    if actions.contains(&NotificationAction::Allow) {
        APPROVAL
    } else if actions.contains(&NotificationAction::Deny) {
        DENY_ONLY
    } else {
        PLAIN
    }
}

pub struct Platform {
    live: Option<Live>,
}

/// Whether this process runs from an app bundle. Only then the centre can be called.
fn in_bundle() -> bool {
    let bundle = NSBundle::mainBundle();
    bundle.bundleIdentifier().is_some() && bundle.bundlePath().to_string().ends_with(".app")
}

fn string(text: &str) -> Retained<NSString> {
    NSString::from_str(text)
}

fn categories() -> Retained<NSSet<UNNotificationCategory>> {
    let allow = UNNotificationAction::actionWithIdentifier_title_options(
        &string(ALLOW),
        &string("Allow"),
        UNNotificationActionOptions::empty(),
    );
    let deny = UNNotificationAction::actionWithIdentifier_title_options(
        &string(DENY),
        &string("Deny"),
        UNNotificationActionOptions::Destructive,
    );
    let approval = UNNotificationCategory::categoryWithIdentifier_actions_intentIdentifiers_options(
        &string(APPROVAL),
        &NSArray::from_retained_slice(&[allow, deny.clone()]),
        &NSArray::new(),
        UNNotificationCategoryOptions::empty(),
    );
    let deny_only =
        UNNotificationCategory::categoryWithIdentifier_actions_intentIdentifiers_options(
            &string(DENY_ONLY),
            &NSArray::from_retained_slice(&[deny]),
            &NSArray::new(),
            UNNotificationCategoryOptions::empty(),
        );
    let plain = UNNotificationCategory::categoryWithIdentifier_actions_intentIdentifiers_options(
        &string(PLAIN),
        &NSArray::new(),
        &NSArray::new(),
        UNNotificationCategoryOptions::empty(),
    );
    NSSet::from_retained_slice(&[approval, deny_only, plain])
}

impl Platform {
    pub fn start(
        on_response: Arc<dyn Fn(Response) + Send + Sync>,
        on_permission: Arc<dyn Fn(Permission) + Send + Sync>,
    ) -> Self {
        if !in_bundle() {
            tracing::info!("this run has no app bundle, so it has no notifications");
            on_permission(Permission::Unavailable);
            return Self { live: None };
        }
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let delegate = Delegate::new(on_response);
        center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        center.setNotificationCategories(&categories());
        let heard = Arc::clone(&on_permission);
        let answer = RcBlock::new(move |granted: Bool, error: *mut NSError| {
            if !error.is_null() {
                tracing::warn!("the system did not take the request for notifications");
            }
            heard(if granted.as_bool() {
                Permission::Granted
            } else {
                Permission::Denied
            });
        });
        center.requestAuthorizationWithOptions_completionHandler(
            UNAuthorizationOptions::Alert
                | UNAuthorizationOptions::Sound
                | UNAuthorizationOptions::Badge,
            &answer,
        );
        Self {
            live: Some(Live {
                center,
                _delegate: delegate,
            }),
        }
    }

    pub fn actions(&self) -> bool {
        self.live.is_some()
    }

    pub fn post(&self, identifier: &str, notification: &HostNotification) {
        let Some(live) = &self.live else {
            return;
        };
        let content = UNMutableNotificationContent::new();
        content.setTitle(&string(&notification.title));
        content.setBody(&string(&notification.body));
        content.setCategoryIdentifier(&string(category(&notification.actions)));
        content.setInterruptionLevel(match notification.urgency {
            Urgency::Low => UNNotificationInterruptionLevel::Passive,
            Urgency::Normal | Urgency::High => UNNotificationInterruptionLevel::Active,
        });
        if notification.urgency != Urgency::Low {
            content.setSound(Some(&UNNotificationSound::defaultSound()));
        }
        // A request with an identifier that exists replaces the notification in place.
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &string(identifier),
            &content,
            None,
        );
        let failed = RcBlock::new(|error: *mut NSError| {
            if !error.is_null() {
                tracing::warn!("the system did not take a notification");
            }
        });
        live.center
            .addNotificationRequest_withCompletionHandler(&request, Some(&failed));
    }

    pub fn withdraw(&self, identifier: &str) {
        let Some(live) = &self.live else {
            return;
        };
        let names = NSArray::from_retained_slice(&[string(identifier)]);
        live.center
            .removeDeliveredNotificationsWithIdentifiers(&names);
        live.center
            .removePendingNotificationRequestsWithIdentifiers(&names);
    }
}
