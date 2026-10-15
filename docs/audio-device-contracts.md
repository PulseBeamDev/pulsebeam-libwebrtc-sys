# Owning Linux audio devices

The pinned SDK declares `AudioDeviceModule::Create` but supplies no definition.
`AudioDeviceModule::create` therefore returns `UnlinkedStaticCreate`; it never
fabricates a nullable native result or substitutes another constructor. Selecting
`ModuleCreate` in a factory/session recipe likewise reports the missing entry
point. This is a pinned SDK interface/implementation gap, not backend absence.

`create_with_helper` invokes the implemented free `CreateAudioDeviceModule` factory
with its default bypass value. `create_with_helper_options` and the typed creation
recipe preserve its explicit `bypass_voice_processing` argument. Nullable outcomes
from this actual factory remain nullable. Linux/backend choice is explicit;
creation never substitutes a headless/default device for failure. A native artifact
is required. Feasible creation reserves production execution and rejects coexistence
with a controlled world before activating a module.

The standalone owner consumes a worker, retains the original environment and
marshals operations/final SDK release to that worker. Accepted raw transport and
observer registrations remain alive until native release. Standalone destruction
stops, unregisters and terminates while dependencies are still alive.

`PeerConnectionFactoryBuilder::audio_device_module` transfers that exact module
and adopts its worker. Do not supply an independent factory worker. No settings
are converted into a replacement creation request. The original environment and
callback products remain retained. After successful handoff, engine lifecycle
owns Stop/Terminate and transport registration; the retained standalone state
only releases its references on the original worker at final destruction.
Stop recording/playout before handoff or replacing/unregistering a standalone
transport. The SDK pointer assignment is not synchronized against active backend
reads; the binding rejects that state before writing it, with a binding admission
error distinct from native status. It does not silently stop a live module.

Native handoff is not a promise to preserve every pre-install setting. Pinned
voice-engine initialization selects default devices, initializes speaker and
microphone, sets stereo from native availability and applies hardware-processing
defaults. It also installs its own transport. Those changes are genuine SDK
behavior, not setter echoes. Factory-bound InstalledAudioDevice and session
controls inspect/configure that exact installed ADM after initialization.

`ProductionSessionBuilder::native_audio_device` retains the creation recipe and
constructs in the actor's actual environment/worker. A sequence-bound standalone
ADM is not made Send or imported into the migratable actor. Device/backend absence,
native status failures, controlled conflicts and owning-worker admission failures
are distinct. Physical capture/playout and callback retirement still require
matching native-runtime evidence; hardware absence cannot supply it.
