//! Params and results of `phone.*`: the pairing of a phone, its
//! devices, its push subscription and whether its app is visible.

use crate::api::{
    common::wire,
    ids::{DeviceId, WindowId},
    time::Timestamp,
};

wire! {
    /// `phone.pair_begin`: a new offer of a pairing. It ends the offer before it.
    #[derive(Default)]
    pub struct PhonePairBegin {}

    /// The offer, also in the projection `phone_pairing`. The result has data so that the CLI can
    /// print the URL and the short code.
    pub struct PhonePairStarted {
        pub url: String,
        pub code: String,
        pub expires_at: Timestamp,
    }

    /// `phone.remove`: removes a paired phone, closes its connections and its streams, and removes
    /// its phone windows.
    pub struct PhoneRemove {
        pub device: DeviceId,
    }

    /// `phone.rename`: the name of a paired phone.
    pub struct PhoneRename {
        pub device: DeviceId,
        /// 1 to 64 characters.
        pub name: String,
    }

    /// `phone.push_subscribe`: the push subscription of the phone that calls. Only a
    /// phone calls it, for itself. A new subscription replaces the old one.
    pub struct PhonePushSubscribe {
        /// The endpoint of the push service. It must be on the allowlist of push services.
        pub endpoint: String,
        /// The public key of the browser (P-256, uncompressed), base64url.
        pub p256dh: String,
        /// The authentication secret of the browser (16 bytes), base64url.
        pub auth: String,
    }

    /// `phone.push_test`: sends the push "Test notification" to a phone.
    pub struct PhonePushTest {
        pub device: DeviceId,
    }

    /// `phone.visible`: the phone app of the phone that calls is open and visible, or not.
    /// The app calls it on each `visibilitychange`. A visible phone gets no push. The
    /// core keeps it in memory for the connection, and the end of the connection makes it false.
    pub struct PhoneVisible {
        pub visible: bool,
    }

    /// The result of `phone.window`: the phone window of the device that calls, on this core.
    /// The core makes the row when the device has none.
    pub struct PhoneWindowResult {
        pub window: WindowId,
    }
}
