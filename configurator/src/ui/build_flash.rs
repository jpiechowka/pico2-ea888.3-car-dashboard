//! Persistent profile choices; recipe export captures accepted state at click time.
use dashboard_build_profiles::{ProfileKind, ResolvedClock, UserObservation, Voltage};
use dashboard_configurator::build_flash::{BuildSelection, repository_toolchain, selected_profile, transfer_estimate};
use dashboard_helper_protocol::{BuildOptions, Mode, Optimization};

use super::*;

fn mhz(hz: u32) -> String { format!("{:.3} MHz", f64::from(hz) / 1_000_000.) }
fn issue(ui: Ui) -> Option<String> {
    if let Some(block) = ui.editor.with(|e| e.action_readiness().block) {
        return Some(dashboard_configurator::build_flash::BuildIssue::Editor(block).message());
    }
    ui.build.with(|s| s.normalized().err().map(|e| e.message()))
}
fn export_recipe(ui: Ui) {
    let prepared = ui
        .build
        .with_untracked(|s| ui.editor.with_untracked(|e| s.prepare(e)))
        .and_then(|recipe| recipe.with_startup(ui.startup.profile.get_untracked()));
    match prepared {
        Ok(recipe) => {
            let script = recipe.powershell();
            match io::download(
                ui,
                "Build-StudioDashboard.ps1",
                "text/plain;charset=utf-8",
                script.as_bytes(),
            ) {
                Ok(()) => {
                    ui.event(Notice::info(
                        Category::HostTools,
                        "build.recipe_exported",
                        None,
                        format!(
                            "Build recipe downloaded with {} bytes of accepted configuration; run it locally to build.",
                            recipe.config_bytes().len()
                        ),
                    ))
                }
                Err(_) => {
                    ui.event(Notice {
                        severity: dashboard_configurator::events::Severity::Error,
                        ..Notice::info(
                            Category::HostTools,
                            "build.recipe_failed",
                            None,
                            "The browser could not download the recipe. Try again.",
                        )
                    })
                }
            }
        }
        Err(e) => {
            ui.event(Notice {
                severity: dashboard_configurator::events::Severity::Warn,
                ..Notice::info(Category::HostTools, "build.recipe_blocked", None, e.message())
            })
        }
    }
}
fn profile_note(s: &BuildSelection) -> &'static str {
    let Some(p) = selected_profile(s) else {
        return "Custom clocks use the firmware's exact PLL and SPI checks. Validation does not establish hardware \
                stability.";
    };
    match (p.observation, p.kind) {
        (Some(UserObservation::Working), _) => {
            "Reported working on this owner's device. Stability and display timing are not qualified for other boards."
        }
        (Some(UserObservation::Failed), _) => {
            "Reported failed on this owner's device. Retained for reproducibility; resolves to 290.4 MHz CPU and 48.4 \
             MHz SPI."
        }
        (_, ProfileKind::Candidate) => {
            "Bus-efficiency candidate: a lower CPU clock can still produce faster SPI through a different divisor. \
             Hardware validation is pending."
        }
        (_, ProfileKind::TimingReference) => {
            "Display timing reference for comparison. Build success does not verify wiring, signal quality or board \
             stability."
        }
        (_, ProfileKind::Default) => {
            "Stock CPU. SPI rounds down to a supported rate: 62.5 MHz requested becomes 37.5 MHz actual."
        }
        _ => "Experimental profile. Hardware stability and display timing remain unverified.",
    }
}
#[component]
fn BuildToggle(
    ui: Ui,
    label: &'static str,
    help: &'static str,
    read: fn(&BuildOptions) -> bool,
    write: fn(&mut BuildOptions, bool),
) -> impl IntoView {
    view! { <label class="build-toggle"><input type="checkbox" prop:checked=move ||ui.build.with(|s|read(&s.options)) on:change=move |e|{let v=event_target_checked(&e);ui.build.update(|s|write(&mut s.options,v));}/><span><strong>{label}</strong><small>{help}</small></span></label> }
}
#[component]
fn ClockFacts(clock: ResolvedClock) -> impl IntoView {
    let estimate = transfer_estimate(&clock);
    let voltage = clock.voltage.millivolts();
    view! {
        <div class="build-clock-grid">
            <div><span>"CPU · actual"</span><strong>{mhz(clock.actual_cpu_hz)}</strong><small>{format!("Requested {}",mhz(clock.requested_cpu_hz))}</small></div>
            <div><span>"DISPLAY SPI · actual"</span><strong>{mhz(clock.actual_spi_hz)}</strong><small>{format!("Requested maximum {}",mhz(clock.requested_spi_hz))}</small></div>
            <div><span>"CORE VOLTAGE · selected"</span><strong>{format!("{:.2} V",f64::from(voltage)/1000.)}</strong><small>"Setpoint, not a measurement"</small></div>
        </div>
        <div class="build-caution" hidden=voltage<=1200><strong>"Above the RP2350 voltage rating"</strong><p>"1.25 V and 1.30 V exceed the documented 1.21 V absolute maximum. Expert mode permits these profiles; it does not make them safe."</p></div>
        {estimate.map(|e|view!{<div class="build-throughput"><div><span>"Full RGB565 payload"</span><strong>{format!("{:.2} ms",e.payload_us as f64/1000.)}</strong></div><div><span>"Payload-only ceiling"</span><strong>{format!("{:.2} / s",e.payloads_per_second_milli as f64/1000.)}</strong></div><p>"Ideal transfer of 153,600 bytes. Rendering, commands, synchronization and scanout are excluded. This is not measured FPS; partial updates have different costs."</p></div>})}
        <details class="build-disclosure"><summary>"Clock divisors and neighboring rates"</summary><p>{format!("SPI = CPU / ({} × ({} + 1)). A lower request can cross a divider boundary and sharply reduce the actual rate.",clock.cpsdvsr,clock.scr)}</p>
            <dl><dt>"At or below request"</dt><dd>{clock.spi_neighbors.below_or_equal.map_or_else(||"Unavailable".into(),|v|mhz(v.actual_hz))}</dd><dt>"Next faster rate"</dt><dd>{clock.spi_neighbors.above.map_or_else(||"None within supported range".into(),|v|format!("{} (request at least {} Hz)",mhz(v.actual_hz),v.request_hz))}</dd><dt>"PLL"</dt><dd>{format!("{:?}",clock.pll)}</dd></dl>
            <p>"132/66 and 140/70 use CPU ÷ 2 for SPI; 280/70 uses CPU ÷ 4. Equal bus rates do not imply equal rendering performance."</p>
            <a href="https://datasheets.raspberrypi.com/rp2350/rp2350-datasheet.pdf" target="_blank" rel="noopener noreferrer">"RP2350 clock and electrical reference ↗"</a>
        </details>
    }
}
#[component]
pub fn BuildFlash(ui: Ui) -> impl IntoView {
    view! {
        <div class="build-studio">
            <div class="section-heading"><div><p class="eyebrow">"YOUR PICO 2"</p><h1>"Build & Flash"</h1></div><span class="chip">"LOCAL BUILD"</span></div>
            <p class="build-intro">"Choose your firmware and clock profile, inspect the resolved settings, then download a build recipe containing this dashboard."</p>
            <section class="build-card" aria-labelledby="build-profile-title"><div class="build-section-title"><span>"01"</span><h2 id="build-profile-title">"Firmware & profile"</h2></div>
                <label class="build-field">"Firmware mode"<select prop:value=move ||ui.build.with(|s|match s.options.mode{Mode::Demo=>"demo",Mode::Vehicle=>"vehicle"}) on:change=move |e|{
                    let v=match event_target_value(&e).as_str(){"demo"=>Some(Mode::Demo),"vehicle"=>Some(Mode::Vehicle),_=>None};if let Some(v)=v{ui.build.update(|s|s.options.mode=v);}
                }><option value="demo">"Demo · synthetic engine values"</option><option value="vehicle">"Vehicle · Longan CAN acquisition"</option></select><small>"Both use the same layout and renderer. Vehicle ECU support still needs captures and hardware validation."</small></label>
                <label class="build-field">"Clock profile"<select prop:value=move ||ui.build.with(BuildSelection::selected_key) on:change=move |e|{let key=event_target_value(&e);ui.build.update(|s|{let _=s.select(&key);});}>{dashboard_build_profiles::catalogue().iter().map(|p|view!{<option value=p.feature>{p.label}</option>}).collect_view()}<option value="custom">"Custom · exact frequencies and voltage"</option></select></label>
                <p class="build-note">{move ||ui.build.with(profile_note)}</p>
                <div class="build-custom" hidden=move ||!ui.build.with(|s|s.custom)>
                    <label class="build-field">"CPU frequency · Hz"<input inputmode="numeric" autocomplete="off" spellcheck="false" prop:value=move ||ui.build.with(|s|s.cpu.text().to_string()) on:input=move |e|{
                        let input=event_target::<web_sys::HtmlInputElement>(&e);let text=io::bounded_input(&input,32);ui.build.update(|s|match text{Some(t)=>s.cpu.set(&t),None=>s.cpu.reject_oversize()});
                    }/><small>"12–300 MHz; requires an exact supported PLL solution."</small></label>
                    <label class="build-field">"SPI maximum · Hz"<input inputmode="numeric" autocomplete="off" spellcheck="false" prop:value=move ||ui.build.with(|s|s.spi.text().to_string()) on:input=move |e|{
                        let input=event_target::<web_sys::HtmlInputElement>(&e);let text=io::bounded_input(&input,32);ui.build.update(|s|match text{Some(t)=>s.spi.set(&t),None=>s.spi.reject_oversize()});
                    }/><small>"1–75 MHz; at most CPU ÷ 2. Actual rate may round down."</small></label>
                    <label class="build-field">"Core voltage setpoint"<select prop:value=move ||ui.build.with(|s|s.voltage.millivolts().to_string()) on:change=move |e|{
                        let v=match event_target_value(&e).as_str(){"1100"=>Some(Voltage::V1_10),"1150"=>Some(Voltage::V1_15),"1200"=>Some(Voltage::V1_20),"1250"=>Some(Voltage::V1_25),"1300"=>Some(Voltage::V1_30),_=>None};if let Some(v)=v{ui.build.update(|s|s.voltage=v);}
                    }><option value="1100">"1.10 V"</option><option value="1150">"1.15 V"</option><option value="1200">"1.20 V"</option><option value="1250">"1.25 V · above rating"</option><option value="1300">"1.30 V · above rating"</option></select></label>
                </div>
                <BuildToggle ui label="Expert mode" help="Required for custom and experimental profiles. Off by default on each page load." read=|o|o.expert write=|o,v|o.expert=v/>
                <div class="build-error" role="status" hidden=move ||ui.build.with(|s|s.normalized().is_ok())>{move ||ui.build.with(|s|s.normalized().err().map(|e|e.message()).unwrap_or_default())}</div>
            </section>
            <section class="build-card" aria-labelledby="build-clocks-title"><div class="build-section-title"><span>"02"</span><h2 id="build-clocks-title">"Resolved hardware settings"</h2></div>
                {move ||ui.build.with(|s|s.normalized().ok().map(|n|view!{<ClockFacts clock=n.clock/>}))}<p hidden=move ||ui.build.with(|s|s.normalized().is_ok())>"Resolve the profile above to inspect its clock and transfer details."</p>
            </section>
            <section class="build-card" aria-labelledby="build-options-title"><div class="build-section-title"><span>"03"</span><h2 id="build-options-title">"Compiler & options"</h2></div>
                <dl class="build-toolchain"><dt>"Repository toolchain"</dt><dd><code>{repository_toolchain().unwrap_or("Unknown")}</code><small>"Expected by this Studio build. Run the local check to detect installed tools."</small></dd><dt>"Target"</dt><dd><code>"thumbv8m.main-none-eabihf"</code><small>"Pico 2 / RP2350 · Cortex-M33 · hardware float"</small></dd></dl>
                <label class="build-field">"Optimization"<select prop:value=move ||ui.build.with(|s|s.options.optimization.cargo_value()) on:change=move |e|{
                    let v=match event_target_value(&e).as_str(){"3"=>Some(Optimization::Speed3),"2"=>Some(Optimization::Speed2),"s"=>Some(Optimization::SizeS),"z"=>Some(Optimization::SizeZ),_=>None};if let Some(v)=v{ui.build.update(|s|s.options.optimization=v);}
                }><option value="3">"3 · favor speed (default)"</option><option value="2">"2 · balanced optimization"</option><option value="s">"s · favor size"</option><option value="z">"z · minimize size"</option></select><small>"Compare performance with the same final firmware workload."</small></label>
                <details class="build-disclosure"><summary>"Rendering and diagnostic options"</summary>
                    <BuildToggle ui label="Simple outline" help="Use the reduced-cost widget outline." read=|o|o.simple_outline write=|o,v|o.simple_outline=v/>
                    <BuildToggle ui label="Serial display path" help="Diagnostic alternative to the pipelined display path." read=|o|o.serial_display write=|o,v|o.serial_display=v/>
                    <BuildToggle ui label="Full-frame transfer" help="Diagnostic mode: send all pixels instead of dirty regions." read=|o|o.full_frame write=|o,v|o.full_frame=v/>
                    <BuildToggle ui label="Frame timing trace" help="Include software frame timing diagnostics." read=|o|o.frame_trace write=|o,v|o.frame_trace=v/>
                    <BuildToggle ui label="TE timing trace" help="Include display synchronization diagnostics." read=|o|o.te_trace write=|o,v|o.te_trace=v/>
                </details>
                <details class="build-disclosure"><summary>"Exact normalized build options"</summary><pre class="build-json">{move ||ui.build.with(|s|match s.normalized(){Ok(n)=>serde_json::to_string_pretty(&n).unwrap_or_else(|_|"Options unavailable".into()),Err(e)=>e.message()})}</pre></details>
            </section>
            <section class="build-card" aria-labelledby="build-run-title"><div class="build-section-title"><span>"04"</span><h2 id="build-run-title">"Build on this computer"</h2></div>
                <p>"The recipe embeds the accepted dashboard, startup profile and selected build settings. Run it in PowerShell 7 with this repository to compile and run the firmware-image gates."</p>
                <button class="primary build-download" disabled=move ||issue(ui).is_some() on:click=move |_|export_recipe(ui)>"Download build recipe ↗"</button>
                <p class="build-error" role="status" hidden=move ||issue(ui).is_none()>{move ||issue(ui).unwrap_or_default()}</p>
                <ol class="build-steps"><li><strong>"Prepare tools once"</strong><code>".\\configurator\\Studio.ps1 -Setup"</code></li><li><strong>"Inspect installed prerequisites"</strong><code>".\\configurator\\Studio.ps1 -Check"</code></li><li><strong>"Run the downloaded recipe"</strong><code>".\\Build-StudioDashboard.ps1 -Repository 'D:\\path\\to\\repository'"</code></li></ol>
                <p class="build-note">"Every run gets a new output name. Reusing a name is rejected to protect existing captures. A changed repository toolchain requires a newly exported recipe."</p>
                <div class="build-connection"><span class="chip">"FLASHING UNAVAILABLE"</span><p>"This recipe builds only. Native connection and verified flashing are being integrated; no device has been selected or written."</p></div>
            </section>
        </div>
    }
}
