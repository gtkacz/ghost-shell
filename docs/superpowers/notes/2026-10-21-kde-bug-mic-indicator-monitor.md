# Draft KDE bug report: microphone indicator flags captures of an output's monitor

A draft for the user to file at https://bugs.kde.org (product **plasma-pa**). Nothing has been submitted. Before filing, confirm the behaviour on screen with the reproduction below.

---

**Product / component:** plasma-pa (Audio Volume), microphone indicator

**Version:** plasma-pa 6.7.5, pulseaudio-qt 1.8.1, PipeWire 1.6.9 with WirePlumber 0.5.18 (pipewire-pulse), Fedora 44, Plasma 6.7.5 on Wayland

**Summary:** The microphone-in-use indicator lists applications that only capture a sink's monitor (what the speakers play), not a microphone.

**Steps to reproduce:**
1. Play some audio.
2. In a terminal, capture the default output's monitor: `parec -d @DEFAULT_SINK@.monitor > /dev/null`
3. Look at the system tray.

**Observed:** The microphone indicator appears and says the application is using the microphone, naming the monitor source (for example "parec is using the microphone (Monitor of …)" when several sources exist). Audio visualisers, which capture the output monitor to draw a spectrum, trigger it the same way.

**Expected:** Capturing a sink's monitor isn't microphone use, so the indicator stays off for it.

**Analysis (from reading the code paths):**
- `MicrophoneIndicator`'s list of recording applications takes every source output whose `VirtualStream` role is false. In pulseaudio-qt that role is `client == PA_INVALID_INDEX`, so any client-owned capture counts.
- The source's `DeviceIndex` is read only to name the device. Nothing checks whether that source is the monitor of a sink, which `pa_source_info.monitor_of_sink != PA_INVALID_INDEX` would tell.
- Corked capture streams count as well, though nothing is flowing.
- The only exclusions are four mixer application ids (`org.PulseAudio.pavucontrol`, `org.gnome.VolumeControl`, `org.kde.kmixd`, `org.kde.plasma-pa`).

**Suggested fix:** Skip source outputs whose source is a monitor of a sink (expose `monitor_of_sink` on pulseaudio-qt's `Source`, or check it in plasma-pa). Optionally, also skip corked streams.
