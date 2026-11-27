import { createContext } from "react";

export interface AvailableDevice {
    model: string;
    /** Name the device is shown by; its model until the user renames it */
    name: string;
    serial_number: string;
    usb: boolean;
    connect: boolean;
    strength: boolean;
}

interface DeviceContextType {
    devices: AvailableDevice[];
    /** Applies a name already stored in the backend to the listed device */
    updateDeviceName: (serial: string, name: string) => void;
}

const defaultValue: DeviceContextType = {
    devices: [],
    updateDeviceName: () => undefined,
};

export const DeviceContext = createContext(defaultValue);
