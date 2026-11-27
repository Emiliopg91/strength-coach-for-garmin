import { BackendClient } from "@/utils/backend/client";
import { BackendListener } from "@/utils/backend/listener";
import { useBackendList } from "@/hooks/useBackendList";
import { useBackendEvent } from "@/hooks/useBackendEvent";
import { useLoadingTask } from "@/hooks/useLoadingTask";
import { DeviceContext } from "@/context/DeviceContext";
import { I18nSettingsContext } from "@/context/I18nSettingsContext";
import { useContext } from "react";
import { DeviceRow } from "./rows/DeviceRow";
import "@/styles/Devices/Devices.css";

export function DeviceList() {
    const { translate } = useContext(I18nSettingsContext);
    const { devices, updateDeviceName } = useContext(DeviceContext);
    const withLoading = useLoadingTask();

    // The context tracks the live connection; how they were registered and the sync marks come
    // from the database, refreshed when those can change
    const [registered, , refreshRegistered] = useBackendList(
        BackendClient.getRegisteredDevices,
        [],
    );
    useBackendEvent<void>(BackendListener.onActivitiesAdded, refreshRegistered);
    useBackendEvent(BackendListener.onDeviceConnected, refreshRegistered);
    useBackendEvent(BackendListener.onDeviceDiscovered, refreshRegistered);

    const registeredBySerial = new Map(
        registered.map((d) => [d.serial_number, d]),
    );

    const rename = (serial: string, name: string) =>
        withLoading(BackendClient.setDeviceName(serial, name)).then(
            (stored) => {
                updateDeviceName(serial, stored);
                return stored;
            },
        );

    const sorted = devices.toSorted((a, b) => a.name.localeCompare(b.name));

    return (
        <div className="list-layer">
            {sorted.length == 0 ? (
                <div className="no-devices">{translate("no_devices")}</div>
            ) : (
                <table id="device-list">
                    <colgroup>
                        <col className="col-device-name" />
                        <col />
                        <col />
                        <col />
                        <col />
                    </colgroup>
                    <thead>
                        <tr>
                            <th className="text-center">{translate("name")}</th>
                            <th className="text-center">
                                {translate("model")}
                            </th>
                            <th className="text-center">
                                {translate("serial_number")}
                            </th>
                            <th className="text-center">
                                {translate("connected_through")}
                            </th>
                            <th className="text-center">
                                {translate("last_import")}
                            </th>
                        </tr>
                    </thead>
                    <tbody>
                        {sorted.map((device) => (
                            <DeviceRow
                                key={device.serial_number}
                                device={device}
                                registered={registeredBySerial.get(
                                    device.serial_number,
                                )}
                                onRename={rename}
                            />
                        ))}
                    </tbody>
                </table>
            )}
        </div>
    );
}
