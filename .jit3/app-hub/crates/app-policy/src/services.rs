//! Host services an app reaches by exact name: `matrix.*` for the person's
//! Matrix account (Rinx) and `octos.*` for the device's assistant.
//!
//! These names are the published contract between a bundle and every host
//! that runs it. A host admits a bundle only when each requested name is in
//! [`crate::KNOWN_CAPABILITIES`] exactly; a prefix is never a grant, so
//! `octos.` or `octos.admin` is an unknown capability. Admission is not
//! dispatch either: the host still intersects the declaration with the
//! services it supports, its own policy and the person's per-instance grant,
//! and checks the resulting lease on every request. Reading assistant
//! history does not imply starting a turn; each service is its own consent.
//!
//! The assistant's model provider, credentials and kernel belong to the host.
//! No `octos.*` service lets an app choose a provider, submit a key or reach
//! the kernel's raw protocol.

/// The assistant services, in the order a store lists them.
pub const OCTOS_SERVICES: &[&str] = &[
    "octos.session.open",
    "octos.session.history",
    "octos.turn.start",
    "octos.turn.interrupt",
];

/// The Matrix services that change something on the person's account or in
/// a room (as opposed to reading). A store summarises them separately so a
/// person sees that the app can act, not only look.
pub const MATRIX_ACTIONS: &[&str] = &[
    "matrix.dm_open",
    "matrix.favorite",
    "matrix.invite",
    "matrix.invite_respond",
    "matrix.join",
    "matrix.low_priority",
    "matrix.mark_unread",
    "matrix.pin",
    "matrix.react",
    "matrix.read_receipt",
    "matrix.reply",
    "matrix.rooms_send",
    "matrix.send_message",
    "matrix.thread_reply",
    "matrix.typing",
];

/// Whether `capability` is one of the exact Matrix or assistant service names.
pub fn is_host_service(capability: &str) -> bool {
    service_words(capability).is_some()
}

/// What a store tells the person a service capability allows, in plain words.
/// `None` for anything that is not an exact service name.
pub fn service_words(capability: &str) -> Option<&'static str> {
    if let Some(words) = octosense_app_contract::palpo::words(capability) { return Some(words); }
    Some(match capability {
        "runtime" => "Inspect available host APIs without gaining access to their data or permissions",
        "auth" => "Connect its own GitHub or Google accounts, or sign in to its developer’s backend, through the host",
        "github" => "Read authorized repositories and ask you to review Markdown commits",
        "gcalendar" => "Read authorized Google calendars and ask you to review event changes",
        "device_calendar" => "Read device calendars you choose and ask you to review event changes",
        "gmail" => "Read authorized Gmail messages, keep reply drafts and request native send review",
        "matrix.account_info" => "See which Matrix account you are using",
        "matrix.device" => "See this device's Matrix session details",
        "matrix.dm_find" => "Find your direct chats with a person",
        "matrix.dm_open" => "Start a direct chat on your Matrix account",
        "matrix.event" => "Read a single message in rooms you allow",
        "matrix.favorite" => "Mark rooms as favourites",
        "matrix.ignored_users" => "See whom you have ignored",
        "matrix.invite" => "Invite people to rooms you allow",
        "matrix.invite_respond" => "Accept or decline room invitations",
        "matrix.invites" => "See your room invitations",
        "matrix.join" => "Join rooms on your Matrix account",
        "matrix.low_priority" => "Mark rooms as low priority",
        "matrix.mark_unread" => "Mark rooms as read or unread",
        "matrix.older_messages" => "Read earlier messages in rooms you allow",
        "matrix.permalink" => "Make links to messages in rooms you allow",
        "matrix.pin" => "Pin messages in rooms you allow",
        "matrix.pinned_events" => "Read pinned messages in rooms you allow",
        "matrix.power_levels" => "See who may do what in rooms you allow",
        "matrix.profile" => "See your Matrix name and picture",
        "matrix.react" => "React to messages in rooms you allow",
        "matrix.read_messages" => "Read messages in rooms you allow",
        "matrix.read_receipt" => "Mark messages as read in rooms you allow",
        "matrix.read_receipts" => "See who has read messages in rooms you allow",
        "matrix.reply" => "Reply to messages in rooms you allow",
        "matrix.room_info" => "See details of rooms you allow",
        "matrix.room_members" => "See the members of rooms you allow",
        "matrix.room_preview" => "Preview rooms before joining",
        "matrix.room_threads" => "List threads in rooms you allow",
        "matrix.rooms_info" => "See details of your rooms",
        "matrix.rooms_list" => "List your rooms",
        "matrix.rooms_messages" => "Read recent messages across rooms you allow",
        "matrix.rooms_search" => "Search your rooms",
        "matrix.rooms_send" => "Send messages to rooms you allow",
        "matrix.search_room" => "Search messages in rooms you allow",
        "matrix.search_rooms" => "Search public rooms",
        "matrix.send_message" => "Send messages in rooms you allow",
        "matrix.space_info" => "See details of your spaces",
        "matrix.space_rooms" => "List the rooms in your spaces",
        "matrix.spaces" => "List your spaces",
        "matrix.successor" => "Follow upgraded rooms to their new room",
        "matrix.thread_replies" => "Read thread replies in rooms you allow",
        "matrix.thread_reply" => "Reply in threads in rooms you allow",
        "matrix.typing" => "Show that you are typing in rooms you allow",
        "matrix.unread" => "See unread counts for your rooms",
        "matrix.user_profile" => "See other people's Matrix names and pictures",
        "octos.session.open" => "Open its own conversation with the assistant",
        "octos.session.history" => "Read its own conversations with the assistant",
        "octos.turn.start" => {
            "Ask the assistant to work for it, using the device's AI settings"
        }
        "octos.turn.interrupt" => "Stop assistant work it started",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::KNOWN_CAPABILITIES;

    #[test]
    fn every_service_name_is_a_known_capability_and_has_words() {
        let services: Vec<&str> = KNOWN_CAPABILITIES
            .iter()
            .copied()
            .filter(|c| c.starts_with("matrix.") || c.starts_with("octos."))
            .collect();
        assert_eq!(services.len(), 49, "45 Matrix and 4 assistant services");
        for capability in services {
            assert!(service_words(capability).is_some(), "{capability} has no words");
        }
        for capability in OCTOS_SERVICES.iter().chain(MATRIX_ACTIONS) {
            assert!(KNOWN_CAPABILITIES.contains(capability), "{capability} is not published");
        }
    }

    #[test]
    fn a_prefix_or_a_near_name_is_not_a_service() {
        for name in [
            "octos.",
            "octos",
            "octos.session",
            "octos.session.open.all",
            "octos.admin",
            "octos.kernel.rpc",
            "OCTOS.turn.start",
            " octos.turn.start",
            "matrix.",
            "matrix.admin",
            "matrix.send_message ",
        ] {
            assert!(!is_host_service(name), "{name:?} must not be a service");
            assert!(!KNOWN_CAPABILITIES.contains(&name), "{name:?} must not be known");
        }
    }

    #[test]
    fn connected_services_have_consent_words_without_implied_grants() {
        for name in ["auth", "github", "gcalendar", "gmail", "device_calendar"] {
            assert!(KNOWN_CAPABILITIES.contains(&name));
            assert!(service_words(name).is_some());
            assert!(!is_host_service(&format!("{name}.*")));
        }
    }
}
