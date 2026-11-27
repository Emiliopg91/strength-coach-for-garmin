import { I18nSettingsContext } from "@/context/I18nSettingsContext";
import { AvailableDevice } from "@/context/DeviceContext";
import { DeviceListItem } from "@/utils/backend/models";
import { TextField } from "@mui/material";
import { KeyboardEvent, useContext, useRef, useState } from "react";

type Props = {
    device: AvailableDevice;
    /** Its stored entry, missing until the list is fetched after the device shows up */
    registered: DeviceListItem | undefined;
    onRename: (serial: string, name: string) => Promise<string>;
};

export function DeviceRow({ device, registered, onRename }: Props) {
    const { translate, formatTimeDate } = useContext(I18nSettingsContext);
    const [name, setName] = useState(device.name);
    // Escape blurs the field too, and that blur must not save what was discarded
    const discarding = useRef(false);

    const save = () => {
        if (discarding.current) {
            discarding.current = false;
            return;
        }
        if (name.trim() == device.name) {
            setName(device.name);
            return;
        }
        // The backend trims it, and gives back the model for a blank one
        onRename(device.serial_number, name)
            .then(setName)
            .catch(() => setName(device.name));
    };

    const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
        if (e.key == "Enter") {
            e.currentTarget.blur();
        } else if (e.key == "Escape") {
            discarding.current = true;
            setName(device.name);
            e.currentTarget.blur();
        }
    };

    // Live connections count too, for a device that just showed up
    const channels = [
        (registered?.usb_registered || device.usb) &&
            translate("connection_usb"),
        (registered?.connect_registered || device.connect) && "Connect",
    ].filter((channel) => channel !== false);
    const lastActivitySync = registered?.last_activity_sync ?? null;

    return (
        <tr>
            <td>
                <TextField
                    variant="standard"
                    size="small"
                    fullWidth
                    value={name}
                    placeholder={device.model}
                    onChange={(e) => setName(e.target.value)}
                    onBlur={save}
                    slotProps={{ htmlInput: { onKeyDown } }}
                />
            </td>
            <td>{device.model}</td>
            <td className="device-serial">{device.serial_number}</td>
            <td>
                {channels.length > 0
                    ? channels.map((channel) => (
                          <div key={channel}>{channel}</div>
                      ))
                    : "—"}
            </td>
            <td>
                {lastActivitySync != null
                    ? formatTimeDate(lastActivitySync)
                    : translate("never")}
            </td>
        </tr>
    );
}
