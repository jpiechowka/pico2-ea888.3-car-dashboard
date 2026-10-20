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
    host: BleHost<'d, C, P>,
}

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
    pub(crate) fn new(host: BleHost<'d, C, P>) -> Self {
        Self { host }
    }

    /// Set the filter accept list for advertising.
    ///
    /// This clears the existing filter accept list on the controller and adds the provided entries.
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

    /// Start advertising with the provided parameters and return a handle to accept connections.
    pub async fn advertise<'k>(
        &mut self,
        params: &AdvertisementParameters,
        data: Advertisement<'k>,
    ) -> Result<Advertiser<'d, C, P>, BleHostError<C::Error>>
    where
        C: for<'t> ControllerCmdSync<LeSetAdvData>
            + ControllerCmdSync<LeSetAdvParams>
            + for<'t> ControllerCmdSync<LeSetAdvEnable>
            + for<'t> ControllerCmdSync<LeSetScanResponseData>,
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

    /// Replace both advertising and scan-response data; empty slices clear old data.
    /// Payloads above 31 bytes are rejected before either controller update.
    /// Does not change other advertising parameters. If no advertising is active, this will not
    /// produce any observable effect. This is typically useful when
    /// implementing a BLE beacon that only broadcasts advertisement data and
    /// does not accept any connections.
    pub async fn update_adv_data<'k>(&mut self, data: Advertisement<'k>) -> Result<(), BleHostError<C::Error>>
    where
        C: for<'t> ControllerCmdSync<LeSetAdvData> + for<'t> ControllerCmdSync<LeSetScanResponseData>,
    {
        self.write_legacy_data(validate_data(data, false)?).await
    }

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

    /// Starts sending BLE advertisements according to the provided config.
    ///
    /// The handles are required to provide the storage while advertising, and
    /// can be created by calling AdvertisementSet::handles(sets).
    ///
    /// When a connection is accepted, its handle is returned and the control
    /// runner stops any remaining sets before admitting another advertiser.
    ///
    /// Returns a handle to accept connections.
    ///
    /// This operation replaces all controller advertising sets after the prior
    /// advertiser has completed cleanup. Raw advertising HCI commands must not
    /// be mixed with this API.
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

    /// Replace supported advertising data and scan-response data for multiple sets.
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

    /// Accept any pending available connection.
    ///
    /// Accepts the next pending connection if there are any.
    pub fn try_accept(&mut self) -> Option<Connection<'d, P>> {
        if let Poll::Ready(conn) = self.host.connections().poll_accept(LeConnRole::Peripheral, &[], None) {
            Some(conn)
        } else {
            None
        }
    }
}

/// Handle to an active advertiser which can accept connections.
pub struct Advertiser<'d, C, P: PacketPool> {
    host: BleHost<'d, C, P>,
    extended: bool,
    done: bool,
}

impl<'d, C: Controller, P: PacketPool> Advertiser<'d, C, P> {
    /// Accept the next peripheral connection for this advertiser.
    ///
    /// Returns Error::Timeout if advertiser stopped.
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
