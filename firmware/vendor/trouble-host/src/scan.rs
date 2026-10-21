//! Legacy/extended scanning leases; advertisement reports are delivered by the host event handler.
//! Dropping a lease requests stop; await `stop` to observe the controller disable acknowledgment.
use core::future::Future;

use bt_hci::cmd::le::{
    LeAddDeviceToFilterAcceptList, LeClearFilterAcceptList, LeSetExtScanEnable, LeSetExtScanParams, LeSetScanEnable,
    LeSetScanParams,
};
use bt_hci::controller::{Controller, ControllerCmdSync};
use bt_hci::param::{FilterDuplicates, ScanningPhy};
pub use bt_hci::param::{LeAdvReportsIter, LeExtAdvReportsIter};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::Instant;

use crate::command::CommandState;
use crate::connection::ScanConfig;
use crate::{bt_hci_duration, BleHostError, Central, PacketPool};

/// A scanner that wraps a central to provide additional functionality
/// around BLE scanning.
///
/// The buffer size can be tuned if in a noisy environment that
/// returns a lot of results.
pub struct Scanner<'d, 'stack, C: Controller, P: PacketPool> {
    /// Exclusively borrowed central used to serialize controller scan setup.
    central: &'d mut Central<'stack, C, P>,
}

impl<'d, 'stack, C: Controller, P: PacketPool> Scanner<'d, 'stack, C, P> {
    /// Returns a scanner borrowing `central`; no controller command is sent yet.
    pub fn new(central: &'d mut Central<'stack, C, P>) -> Self {
        Self { central }
    }

    /// Starts extended scanning with `config` PHYs, filtering, duplicate policy and timeout.
    /// Returns an active lease, or the first controller/configuration error. Reports do not stop
    /// the scan. Zero timeout means continuous scanning; awaiting the lease observes the controller
    /// timeout event. Cancellation/error requests asynchronous cleanup; the runner must keep polling.
    pub async fn scan_ext(
        &mut self,
        config: &ScanConfig<'_>,
    ) -> Result<ScanSession<'stack, true>, BleHostError<C::Error>>
    where
        C: ControllerCmdSync<LeSetExtScanEnable>
            + ControllerCmdSync<LeSetExtScanParams>
            + ControllerCmdSync<LeClearFilterAcceptList>
            + ControllerCmdSync<LeAddDeviceToFilterAcceptList>,
    {
        let host = self.central.host;
        let drop = crate::host::OnDrop::new(|| {
            host.scan_command_state().cancel(true);
        });
        host.request_operation(host.scan_command_state(), true).await;
        self.central.set_accept_filter(config.filter_accept_list).await?;

        let scanning = ScanningPhy {
            active_scan: config.active,
            scan_interval: bt_hci_duration(config.interval),
            scan_window: bt_hci_duration(config.window),
        };
        let phy_params = crate::central::create_phy_params(scanning, config.phys);
        let host = &self.central.host;
        host.command(LeSetExtScanParams::new(
            host.own_addr_kind(),
            if config.filter_accept_list.is_empty() {
                bt_hci::param::ScanningFilterPolicy::BasicUnfiltered
            } else {
                bt_hci::param::ScanningFilterPolicy::BasicFiltered
            },
            phy_params,
        ))
        .await?;

        host.scan_timeout().reset();
        host.command(LeSetExtScanEnable::new(
            true,
            config.filter_duplicates,
            bt_hci_duration(config.timeout),
            bt_hci::param::Duration::from_secs(0),
        ))
        .await?;
        drop.defuse();
        Ok(ScanSession {
            command_state: self.central.host.scan_command_state(),
            deadline: if config.timeout.as_ticks() == 0 {
                None
            } else {
                Some(Instant::now() + config.timeout)
            },
            timeout: host.scan_timeout(),
            done: false,
        })
    }

    /// Starts legacy scanning with `config` interval/window, active mode and filtering.
    /// Returns an active lease or the first controller error; reports do not stop scanning.
    /// This implementation stores but does not enforce `config.timeout` for legacy scans: bound
    /// the session externally and await `stop`. Cancellation/error only requests runner cleanup.
    pub async fn scan(&mut self, config: &ScanConfig<'_>) -> Result<ScanSession<'stack, false>, BleHostError<C::Error>>
    where
        C: ControllerCmdSync<LeSetScanParams>
            + ControllerCmdSync<LeSetScanEnable>
            + ControllerCmdSync<LeClearFilterAcceptList>
            + ControllerCmdSync<LeAddDeviceToFilterAcceptList>,
    {
        let host = self.central.host;
        let drop = crate::host::OnDrop::new(|| {
            host.scan_command_state().cancel(false);
        });
        host.request_operation(host.scan_command_state(), false).await;

        if !config.filter_accept_list.is_empty() {
            self.central.set_accept_filter(config.filter_accept_list).await?;
        }

        let params = LeSetScanParams::new(
            if config.active {
                bt_hci::param::LeScanKind::Active
            } else {
                bt_hci::param::LeScanKind::Passive
            },
            bt_hci_duration(config.interval),
            bt_hci_duration(config.window),
            host.own_addr_kind(),
            if config.filter_accept_list.is_empty() {
                bt_hci::param::ScanningFilterPolicy::BasicUnfiltered
            } else {
                bt_hci::param::ScanningFilterPolicy::BasicFiltered
            },
        );
        host.command(params).await?;

        let filter_duplicates = !matches!(config.filter_duplicates, FilterDuplicates::Disabled);
        host.command(LeSetScanEnable::new(true, filter_duplicates)).await?;
        drop.defuse();
        Ok(ScanSession {
            command_state: self.central.host.scan_command_state(),
            deadline: if config.timeout.as_ticks() == 0 {
                None
            } else {
                Some(Instant::now() + config.timeout)
            },
            timeout: host.scan_timeout(),
            done: false,
        })
    }
}

/// Active scan lease; `EXTENDED` selects which controller disable command cleanup sends.
pub struct ScanSession<'d, const EXTENDED: bool> {
    /// Runner-owned state whose idle transition acknowledges cleanup.
    command_state: &'d CommandState<bool>,
    /// Recorded host deadline (`None` for continuous); currently not polled/enforced.
    deadline: Option<Instant>,
    /// Controller extended-scan timeout event, consumed when the lease is polled.
    timeout: &'d Signal<NoopRawMutex, ()>,
    /// Explicit stop observed idle; suppresses a redundant cancellation on drop.
    done: bool,
}

impl<const EXTENDED: bool> ScanSession<'_, EXTENDED> {
    /// Consumes the lease and waits for the runner to acknowledge controller scan disable.
    /// Dropping this future retains the cancellation request, but does not prove cleanup completed.
    pub async fn stop(mut self) {
        self.command_state.cancel(EXTENDED);
        self.command_state.wait_idle().await;
        self.done = true;
    }
}

impl<const EXTENDED: bool> Drop for ScanSession<'_, EXTENDED> {
    /// Requests stop unless explicit `stop` already observed idle; does not wait for hardware.
    fn drop(&mut self) {
        if !self.done {
            self.command_state.cancel(EXTENDED);
        }
    }
}

impl<'d> Future for ScanSession<'d, true> {
    /// Completion indicates receipt of the controller extended-scan timeout event.
    type Output = ();

    /// Registers `cx` for the timeout signal and returns ready once that signal is consumed.
    /// Polling `self` does not deliver reports or acknowledge explicit stop.
    fn poll(self: core::pin::Pin<&mut Self>, cx: &mut core::task::Context<'_>) -> core::task::Poll<Self::Output> {
        let this = self.get_mut();
        core::pin::pin!(this.timeout.wait()).poll(cx)
    }
}
