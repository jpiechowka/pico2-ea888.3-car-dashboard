//! Functionality for the BLE peripheral role.
use core::task::Poll;

use bt_hci::cmd::le::{
    LeAddDeviceToFilterAcceptList, LeClearFilterAcceptList, LeSetAdvData, LeSetAdvEnable, LeSetAdvParams,
    LeSetScanResponseData,
};
#[cfg(feature = "extended-advertising")]
use bt_hci::cmd::le::{
    LeClearAdvSets, LeReadNumberOfSupportedAdvSets, LeSetAdvSetRandomAddr, LeSetExtAdvData, LeSetExtAdvEnable,
    LeSetExtAdvParams, LeSetExtScanResponseData,
};
use bt_hci::controller::{Controller, ControllerCmdSync};
#[cfg(feature = "extended-advertising")]
use bt_hci::param::Operation;
use bt_hci::param::{AddrKind, AdvChannelMap, AdvHandle, AdvKind, AdvSet, BdAddr, LeConnRole};
use embassy_futures::select::{select, Either};

#[cfg(feature = "extended-advertising")]
use crate::advertise::AdvertisementSet;
use crate::advertise::{Advertisement, AdvertisementParameters, RawAdvertisement};
#[cfg(feature = "extended-advertising")]
use crate::bt_hci_ext_duration;
use crate::connection::Connection;
use crate::host::BleHost;
use crate::{bt_hci_duration, Address, BleHostError, Error, PacketPool};

/// Type which implements the BLE peripheral role.
///
/// High-level advertisers own all controller advertising sets. Do not mix raw
/// advertising HCI commands with these operations; cleanup disables all sets.
pub struct Peripheral<'d, C, P: PacketPool> {
    /// Shared controller/state handle; runner must keep processing events and cleanup commands.
    host: BleHost<'d, C, P>,
}

/// Returns raw `data` after validating property combinations and complete-command payload lengths.
/// `extended` permits extended PDUs; legacy payloads cap at31 bytes, extended at251.
/// Returns InvalidValue for invalid contents, ExtendedAdvertisingNotSupported for extended PDUs in legacy mode.
fn validate_data(data: Advertisement<'_>, extended: bool) -> Result<RawAdvertisement<'_>, Error> {
    if !data.is_valid() {
        return Err(Error::InvalidValue);
    }
    let data: RawAdvertisement = data.into();
    if !extended && !data.props.legacy_adv() {
        return Err(Error::ExtendedAdvertisingNotSupported);
    }
    // This API supplies one complete HCI data command, not a fragmented upload.
    let limit = if data.props.legacy_adv() { 31 } else { 251 };
    if data.adv_data.len() > limit || data.scan_data.len() > limit {
        return Err(Error::InvalidValue);
    }
    Ok(data)
}

impl<'d, C: Controller, P: PacketPool> Peripheral<'d, C, P> {
    /// Returns the peripheral role borrowing `host`; does not start advertising.
    pub(crate) fn new(host: BleHost<'d, C, P>) -> Self {
        Self { host }
    }

    /// Set the filter accept list for advertising.
    ///
    /// Clears the shared controller list, then adds `filter_accept_list` in order.
    /// Returns Ok after all commands, or the first controller/host error; errors and cancellation leave partial updates.
    /// Must be called before starting advertising (not while advertising is active).
    pub async fn set_filter_accept_list(&mut self, filter_accept_list: &[Address]) -> Result<(), BleHostError<C::Error>>
    where
        C: ControllerCmdSync<LeClearFilterAcceptList> + ControllerCmdSync<LeAddDeviceToFilterAcceptList>,
    {
        let host = &self.host;
        host.command(LeClearFilterAcceptList::new()).await?;
        for entry in filter_accept_list {
            host.command(LeAddDeviceToFilterAcceptList::new(entry.kind, entry.addr))
                .await?;
        }
        Ok(())
    }

    /// Starts legacy advertising using `params` and validated `data`; returns the active advertiser or command error.
    /// Waits for prior advertiser cleanup before changing controller state. Once admitted, failure/cancellation
    /// queues a disable for the runner; dropping a pending admission cannot cancel another operation.
    /// Legacy commands do not enforce `params.timeout`/max_events; PHY, TX power and fragmentation are extended-only.
    /// Interval/timeout conversion panics when its HCI tick count overflows; values floor to the HCI unit.
    /// Host advertising-state capacity must include one set or initialization panics.
    pub async fn advertise<'k>(
        &mut self,
        params: &AdvertisementParameters,
        data: Advertisement<'k>,
    ) -> Result<Advertiser<'d, C, P>, BleHostError<C::Error>>
    where
        C: ControllerCmdSync<LeSetAdvData>
            + ControllerCmdSync<LeSetAdvParams>
            + ControllerCmdSync<LeSetAdvEnable>
            + ControllerCmdSync<LeSetScanResponseData>,
    {
        let host = &self.host;

        let data = validate_data(data, false)?;

        // Ensure no other advertise ongoing.
        host.request_operation(host.advertise_command_state(), false).await;
        // Only an admitted operation owns cancellation of the controller.
        let drop = crate::host::OnDrop::new(|| {
            host.advertise_command_state().cancel(false);
        });

        // Clear current advertising terminations
        host.advertise_state().reset();

        let kind = match (
            data.props.connectable_adv(),
            data.props.scannable_adv(),
            data.props.high_duty_cycle_directed_connectable_adv(),
        ) {
            (true, true, _) => AdvKind::AdvInd,
            (true, false, true) => AdvKind::AdvDirectIndHigh,
            (true, false, false) => AdvKind::AdvDirectIndLow,
            (false, true, _) => AdvKind::AdvScanInd,
            (false, false, _) => AdvKind::AdvNonconnInd,
        };
        let peer = data.peer.unwrap_or(Address::new(AddrKind::PUBLIC, BdAddr::default()));

        host.command(LeSetAdvParams::new(
            bt_hci_duration(params.interval_min),
            bt_hci_duration(params.interval_max),
            kind,
            params.own_addr_kind.unwrap_or_else(|| host.own_addr_kind()),
            peer.kind,
            peer.addr,
            params.channel_map.unwrap_or(AdvChannelMap::ALL),
            params.filter_policy,
        ))
        .await?;

        self.write_legacy_data(data).await?;

        let advset: [AdvSet; 1] = [AdvSet {
            adv_handle: AdvHandle::new(0),
            duration: bt_hci_duration(params.timeout.unwrap_or(embassy_time::Duration::from_micros(0))),
            max_ext_adv_events: 0,
        }];

        trace!("[host] enabling advertising");
        host.advertise_state().start(&advset[..]);
        host.command(LeSetAdvEnable::new(true)).await?;
        drop.defuse();
        Ok(Advertiser {
            host: self.host,
            extended: false,
            done: false,
        })
    }

    /// Replaces both fields from `data`; empty slices clear old data. Returns Ok after both commands or first error.
    /// Cancellation/error after the first command leaves a partial update and does not stop advertising.
    /// Payloads above 31 bytes are rejected before either controller update.
    /// Does not change other advertising parameters. If no advertising is active, this will not
    /// produce any observable effect. This is typically useful when
    /// implementing a BLE beacon that only broadcasts advertisement data and
    /// does not accept any connections.
    pub async fn update_adv_data<'k>(&mut self, data: Advertisement<'k>) -> Result<(), BleHostError<C::Error>>
    where
        C: ControllerCmdSync<LeSetAdvData> + ControllerCmdSync<LeSetScanResponseData>,
    {
        self.write_legacy_data(validate_data(data, false)?).await
    }

    /// Sends validated `data` in two zero-padded31-byte HCI commands; returns first error or Ok.
    /// Oversized fields panic before sending. Cancellation after advertising-data write can leave old scan data.
    async fn write_legacy_data(&self, data: RawAdvertisement<'_>) -> Result<(), BleHostError<C::Error>>
    where
        C: ControllerCmdSync<LeSetAdvData> + ControllerCmdSync<LeSetScanResponseData>,
    {
        let mut adv = [0; 31];
        let mut scan = [0; 31];
        adv[..data.adv_data.len()].copy_from_slice(data.adv_data);
        scan[..data.scan_data.len()].copy_from_slice(data.scan_data);
        self.host
            .command(LeSetAdvData::new(data.adv_data.len() as u8, adv))
            .await?;
        self.host
            .command(LeSetScanResponseData::new(data.scan_data.len() as u8, scan))
            .await?;
        Ok(())
    }

    /// Replaces all controller sets with `sets`, assigning sequential IDs into `handles`; returns an advertiser.
    /// Requires 1..=63 equally many sets/handles and sufficient controller/host capacity; otherwise returns
    /// InvalidValue/InsufficientSpace before admission. Each field is limited to31 legacy or251 extended bytes.
    /// Prior cleanup completes before set clearing. Controller failure or cancellation after admission queues
    /// runner cleanup; `handles` may be partially updated. Caller data is copied by completed HCI commands.
    /// Interval/timeout conversion panics when its HCI tick count overflows.
    /// Accepted connections trigger remaining-set cleanup; raw HCI advertising must not be mixed with this API.
    #[cfg(feature = "extended-advertising")]
    pub async fn advertise_ext<'k>(
        &mut self,
        sets: &[AdvertisementSet<'k>],
        handles: &mut [AdvSet],
    ) -> Result<Advertiser<'d, C, P>, BleHostError<C::Error>>
    where
        C: for<'t> ControllerCmdSync<LeSetExtAdvData<'t>>
            + ControllerCmdSync<LeClearAdvSets>
            + ControllerCmdSync<LeSetExtAdvParams>
            + ControllerCmdSync<LeSetAdvSetRandomAddr>
            + ControllerCmdSync<LeReadNumberOfSupportedAdvSets>
            + for<'t> ControllerCmdSync<LeSetExtAdvEnable<'t>>
            + for<'t> ControllerCmdSync<LeSetExtScanResponseData<'t>>,
    {
        // HCI command length is 2 + 4 * count, so at most 63 sets fit.
        if sets.is_empty() || sets.len() != handles.len() || sets.len() > 63 {
            return Err(Error::InvalidValue.into());
        }
        let host = &self.host;

        // Check all sets are valid
        for set in sets.iter() {
            validate_data(set.data, true)?;
        }

        // Check host supports the required advertisement sets
        {
            let result = host.command(LeReadNumberOfSupportedAdvSets::new()).await?;
            if usize::from(result) < sets.len() || host.advertise_state().len() < sets.len() {
                return Err(Error::InsufficientSpace.into());
            }
        }

        // Ensure no other advertise ongoing.
        host.request_operation(host.advertise_command_state(), true).await;
        let drop = crate::host::OnDrop::new(|| {
            host.advertise_command_state().cancel(true);
        });

        // Clear current advertising terminations
        host.advertise_state().reset();

        // The operation owns all sets and prior disable has completed. Removing
        // retained data permits changing to a PDU type that cannot carry it.
        host.command(LeClearAdvSets::new()).await?;

        for (i, set) in sets.iter().enumerate() {
            let handle = AdvHandle::new(i as u8);
            let data: RawAdvertisement<'k> = set.data.into();
            let params = set.params;
            let peer = data.peer.unwrap_or_default();
            let own_addr_kind = params.own_addr_kind.unwrap_or_else(|| host.own_addr_kind());
            let random_addr = if let Some(addr) = set.address {
                Some(addr)
            } else {
                host.address().as_ref().map(|a| a.addr)
            };

            host.command(LeSetExtAdvParams::new(
                handle,
                data.props,
                bt_hci_ext_duration(params.interval_min),
                bt_hci_ext_duration(params.interval_max),
                params.channel_map.unwrap_or(AdvChannelMap::ALL),
                own_addr_kind,
                peer.kind,
                peer.addr,
                params.filter_policy,
                params.tx_power as i8,
                params.primary_phy,
                0,
                params.secondary_phy,
                0,
                false,
            ))
            .await?;

            if let Some(addr) = random_addr {
                host.command(LeSetAdvSetRandomAddr::new(handle, addr)).await?;
            }

            self.write_extended_data(handle, data, params.fragment).await?;
            handles[i].adv_handle = handle;
            handles[i].duration = bt_hci_duration(set.params.timeout.unwrap_or(embassy_time::Duration::from_micros(0)));
            handles[i].max_ext_adv_events = set.params.max_events.unwrap_or(0);
        }

        trace!("[host] enabling extended advertising");
        host.advertise_state().start(handles);
        host.command(LeSetExtAdvEnable::new(true, handles)).await?;
        drop.defuse();
        Ok(Advertiser {
            host: self.host,
            extended: true,
            done: false,
        })
    }

    /// Updates payloads from `sets` for matching existing `handles`; returns Ok or the first validation/command error.
    /// Requires equal lengths<=63, distinct handle IDs<=0xEF and valid payloads; an empty batch succeeds.
    /// `handles` IDs are read, not reassigned; cancellation may leave some fields/sets updated.
    /// Empty slices request deletion; a controller may reject deletion of scan
    /// data while a scannable extended set is enabled. Each field is limited to
    /// 251 bytes (31 for legacy sets); fragmented HCI uploads are not implemented.
    /// Updates across commands are not atomic if the controller rejects a command.
    /// Does not change any other advertising parameters. If
    /// no advertising is active, this will not produce any observable effect.
    /// This is typically useful when implementing a BLE beacon that only
    /// broadcasts advertisement data and does not accept any connections.
    #[cfg(feature = "extended-advertising")]
    pub async fn update_adv_data_ext<'k>(
        &mut self,
        sets: &[AdvertisementSet<'k>],
        handles: &mut [AdvSet],
    ) -> Result<(), BleHostError<C::Error>>
    where
        C: for<'t> ControllerCmdSync<LeSetExtAdvData<'t>> + for<'t> ControllerCmdSync<LeSetExtScanResponseData<'t>>,
    {
        if sets.len() != handles.len() || sets.len() > 63 {
            return Err(Error::InvalidValue.into());
        }
        // Validate the whole batch before making any partial controller update.
        for (i, set) in sets.iter().enumerate() {
            validate_data(set.data, true)?;
            if handles[i].adv_handle.as_raw() > 0xef
                || handles[..i]
                    .iter()
                    .any(|other| other.adv_handle == handles[i].adv_handle)
            {
                return Err(Error::InvalidValue.into());
            }
        }
        for (i, set) in sets.iter().enumerate() {
            let handle = handles[i].adv_handle;
            let data: RawAdvertisement<'k> = set.data.into();
            self.write_extended_data(handle, data, set.params.fragment).await?;
        }
        Ok(())
    }

    #[cfg(feature = "extended-advertising")]
    /// Writes complete validated `data` for `handle`, using `fragment` as the controller fragmentation preference.
    /// Skips unsupported advertising fields but always sends scan data; returns Ok or first command error.
    /// Cancellation/error between commands leaves partial state; no rollback is attempted.
    async fn write_extended_data(
        &self,
        handle: AdvHandle,
        data: RawAdvertisement<'_>,
        fragment: bool,
    ) -> Result<(), BleHostError<C::Error>>
    where
        C: for<'t> ControllerCmdSync<LeSetExtAdvData<'t>> + for<'t> ControllerCmdSync<LeSetExtScanResponseData<'t>>,
    {
        // HCI 7.8.54 rejects this command even with no bytes if the PDU type
        // cannot carry advertising data. Scan-response deletion is permitted.
        let supports_adv_data = if data.props.legacy_adv() {
            !data.props.directed_adv()
        } else {
            !data.props.scannable_adv()
        };
        if supports_adv_data {
            self.host
                .command(LeSetExtAdvData::new(
                    handle,
                    Operation::Complete,
                    fragment,
                    data.adv_data,
                ))
                .await?;
        }
        self.host
            .command(LeSetExtScanResponseData::new(
                handle,
                Operation::Complete,
                fragment,
                data.scan_data,
            ))
            .await?;
        Ok(())
    }

    /// Returns the next unaccepted peripheral connection, or None without waiting; no peer filter is applied.
    pub fn try_accept(&mut self) -> Option<Connection<'d, P>> {
        if let Poll::Ready(conn) = self.host.connections().poll_accept(LeConnRole::Peripheral, &[], None) {
            Some(conn)
        } else {
            None
        }
    }
}

/// Active advertiser lease. Drop requests asynchronous disable; it does not wait for controller acknowledgment.
pub struct Advertiser<'d, C, P: PacketPool> {
    /// Shared controller/state handle; runner must keep processing events and cleanup commands.
    host: BleHost<'d, C, P>,
    /// Selects extended-all-sets disable versus legacy disable during cleanup.
    extended: bool,
    /// accept completed; legacy termination can release admission without another disable.
    done: bool,
}

impl<'d, C: Controller, P: PacketPool> Advertiser<'d, C, P> {
    /// Consumes the lease and returns the next peripheral connection or Timeout when all sets have terminated.
    /// This does not itself run a timeout timer. Cancellation drops the lease and requests runner cleanup.
    /// For extended sets, successful acceptance also queues disable of any remaining sets.
    pub async fn accept(mut self) -> Result<Connection<'d, P>, Error> {
        let result = match select(
            self.host.connections().accept(LeConnRole::Peripheral, &[]),
            self.host.advertise_state().wait(),
        )
        .await
        {
            Either::First(conn) => Ok(conn),
            Either::Second(_) => Err(Error::Timeout),
        };
        self.done = true;
        result
    }
}

impl<C, P: PacketPool> Drop for Advertiser<'_, C, P> {
    /// Requests mode-appropriate cleanup, or releases an already-completed legacy operation; never waits.
    fn drop(&mut self) {
        // One connection terminates only its own extended set. The control
        // runner must disable the remaining sets before releasing admission.
        if !self.done || self.extended {
            self.host.advertise_command_state().cancel(self.extended);
        } else {
            self.host.advertise_command_state().canceled();
        }
    }
}
