/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::compartments::{AlreadyInCompartment, InCompartment};
use crate::dom::bindings::callback::ExceptionHandling;
use crate::dom::bindings::cell::DomRefCell;
use crate::dom::bindings::codegen::Bindings::HTMLMediaElementBinding::HTMLMediaElementMethods;
use crate::dom::bindings::codegen::Bindings::MediaMetadataBinding::MediaMetadataInit;
use crate::dom::bindings::codegen::Bindings::MediaMetadataBinding::MediaMetadataMethods;
use crate::dom::bindings::codegen::Bindings::MediaSessionBinding;
use crate::dom::bindings::codegen::Bindings::MediaSessionBinding::MediaSessionAction;
use crate::dom::bindings::codegen::Bindings::MediaSessionBinding::MediaSessionActionHandler;
use crate::dom::bindings::codegen::Bindings::MediaSessionBinding::MediaSessionMethods;
use crate::dom::bindings::codegen::Bindings::MediaSessionBinding::MediaSessionPlaybackState;
use crate::dom::bindings::reflector::{reflect_dom_object, DomObject, Reflector};
use crate::dom::bindings::root::{DomRoot, MutNullableDom};
use crate::dom::bindings::str::DOMString;
use crate::dom::htmlmediaelement::HTMLMediaElement;
use crate::dom::mediametadata::MediaMetadata;
use crate::dom::window::Window;
use dom_struct::dom_struct;
use embedder_traits::MediaMetadata as EmbedderMediaMetadata;
use embedder_traits::MediaSessionEvent;
use script_traits::MediaSessionActionType;
use script_traits::ScriptMsg;
use std::collections::HashMap;
use std::rc::Rc;

#[dom_struct]
pub struct MediaSession {
    reflector_: Reflector,
    /// https://w3c.github.io/mediasession/#dom-mediasession-metadata
    #[ignore_malloc_size_of = "defined in embedder_traits"]
    metadata: DomRefCell<Option<EmbedderMediaMetadata>>,
    /// https://w3c.github.io/mediasession/#dom-mediasession-playbackstate
    playback_state: DomRefCell<MediaSessionPlaybackState>,
    /// https://w3c.github.io/mediasession/#supported-media-session-actions
    #[ignore_malloc_size_of = "Rc"]
    action_handlers: DomRefCell<HashMap<MediaSessionActionType, Rc<MediaSessionActionHandler>>>,
    /// The media instance controlled by this media session.
    /// For now only HTMLMediaElements are controlled by media sessions.
    media_instance: MutNullableDom<HTMLMediaElement>,
}

impl MediaSession {
    #[allow(unrooted_must_root)]
    fn new_inherited() -> MediaSession {
        let media_session = MediaSession {
            reflector_: Reflector::new(),
            metadata: DomRefCell::new(None),
            playback_state: DomRefCell::new(MediaSessionPlaybackState::None),
            action_handlers: DomRefCell::new(HashMap::new()),
            media_instance: Default::default(),
        };
        media_session
    }

    pub fn new(window: &Window) -> DomRoot<MediaSession> {
        reflect_dom_object(
            Box::new(MediaSession::new_inherited()),
            window,
            MediaSessionBinding::Wrap,
        )
    }

    pub fn register_media_instance(&self, media_instance: &HTMLMediaElement) {
        self.media_instance.set(Some(media_instance));
    }

    pub fn handle_action(&self, action: MediaSessionActionType) {
        debug!("Handle media session action {:?}", action);

        if let Some(handler) = self.action_handlers.borrow().get(&action) {
            if handler.Call__(ExceptionHandling::Report).is_err() {
                warn!("Error calling MediaSessionActionHandler callback");
            }
            return;
        }

        // Default action.
        if let Some(media) = self.media_instance.get() {
            match action {
                MediaSessionActionType::Play => {
                    let in_compartment_proof = AlreadyInCompartment::assert(&self.global());
                    media.Play(InCompartment::Already(&in_compartment_proof));
                },
                MediaSessionActionType::Pause => {
                    media.Pause();
                },
                MediaSessionActionType::SeekBackward => {},
                MediaSessionActionType::SeekForward => {},
                MediaSessionActionType::PreviousTrack => {},
                MediaSessionActionType::NextTrack => {},
                MediaSessionActionType::SkipAd => {},
                MediaSessionActionType::Stop => {},
                MediaSessionActionType::SeekTo => {},
            }
        }
    }

    pub fn send_event(&self, event: MediaSessionEvent) {
        let global = self.global();
        let window = global.as_window();
        let pipeline_id = window
            .pipeline_id()
            .expect("Cannot send media session event outside of a pipeline");
        window.send_to_constellation(ScriptMsg::MediaSessionEvent(pipeline_id, event));
    }

    pub fn update_title(&self, title: String) {
        let mut metadata = self.metadata.borrow_mut();
        if let Some(ref mut metadata) = *metadata {
            // We only update the title with the data provided by the media
            // player and iff the user did not provide a title.
            if !metadata.title.is_empty() {
                return;
            }
            metadata.title = title;
        } else {
            *metadata = Some(EmbedderMediaMetadata::new(title));
        }
        self.send_event(MediaSessionEvent::SetMetadata(
            metadata.as_ref().unwrap().clone(),
        ));
    }
}

impl MediaSessionMethods for MediaSession {
    /// https://w3c.github.io/mediasession/#dom-mediasession-metadata
    fn GetMetadata(&self) -> Option<DomRoot<MediaMetadata>> {
        if let Some(ref metadata) = *self.metadata.borrow() {
            let mut init = MediaMetadataInit::empty();
            init.title = DOMString::from_string(metadata.title.clone());
            init.artist = DOMString::from_string(metadata.artist.clone());
            init.album = DOMString::from_string(metadata.album.clone());
            let global = self.global();
            Some(MediaMetadata::new(&global.as_window(), &init))
        } else {
            None
        }
    }

    /// https://w3c.github.io/mediasession/#dom-mediasession-metadata
    fn SetMetadata(&self, metadata: Option<&MediaMetadata>) {
        if let Some(ref metadata) = metadata {
            metadata.set_session(self);
        }

        let global = self.global();
        let window = global.as_window();
        let _metadata = match metadata {
            Some(m) => {
                let title = if m.Title().is_empty() {
                    window.get_url().into_string()
                } else {
                    m.Title().into()
                };
                EmbedderMediaMetadata {
                    title,
                    artist: m.Artist().into(),
                    album: m.Album().into(),
                }
            },
            None => EmbedderMediaMetadata::new(window.get_url().into_string()),
        };

        *self.metadata.borrow_mut() = Some(_metadata.clone());

        self.send_event(MediaSessionEvent::SetMetadata(_metadata));
    }

    /// https://w3c.github.io/mediasession/#dom-mediasession-playbackstate
    fn PlaybackState(&self) -> MediaSessionPlaybackState {
        *self.playback_state.borrow()
    }

    /// https://w3c.github.io/mediasession/#dom-mediasession-playbackstate
    fn SetPlaybackState(&self, state: MediaSessionPlaybackState) {
        *self.playback_state.borrow_mut() = state;
    }

    /// https://w3c.github.io/mediasession/#update-action-handler-algorithm
    fn SetActionHandler(
        &self,
        action: MediaSessionAction,
        handler: Option<Rc<MediaSessionActionHandler>>,
    ) {
        match handler {
            Some(handler) => self
                .action_handlers
                .borrow_mut()
                .insert(action.into(), handler.clone()),
            None => self.action_handlers.borrow_mut().remove(&action.into()),
        };
    }
}

impl From<MediaSessionAction> for MediaSessionActionType {
    fn from(action: MediaSessionAction) -> MediaSessionActionType {
        match action {
            MediaSessionAction::Play => MediaSessionActionType::Play,
            MediaSessionAction::Pause => MediaSessionActionType::Pause,
            MediaSessionAction::Seekbackward => MediaSessionActionType::SeekBackward,
            MediaSessionAction::Seekforward => MediaSessionActionType::SeekForward,
            MediaSessionAction::Previoustrack => MediaSessionActionType::PreviousTrack,
            MediaSessionAction::Nexttrack => MediaSessionActionType::NextTrack,
            MediaSessionAction::Skipad => MediaSessionActionType::SkipAd,
            MediaSessionAction::Stop => MediaSessionActionType::Stop,
            MediaSessionAction::Seekto => MediaSessionActionType::SeekTo,
        }
    }
}