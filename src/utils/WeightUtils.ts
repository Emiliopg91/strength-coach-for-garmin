import { WeightUnit } from "@/utils/backend/models";

export class WeightUtils {
    /** Step weight inputs move by, in the display unit: 0.5 kg or 1 lb. */
    public static step(unit: WeightUnit): number {
        return unit == WeightUnit.Pounds ? 1 : 0.5;
    }

    /** `value` rounded to the nearest input step of `unit`. */
    public static snap(value: number, unit: WeightUnit): number {
        const step = WeightUtils.step(unit);
        return WeightUtils.roundTo(value, step);
    }

    /**
     * `value` as a weight input shows it. Weights are stored to 0.1 kg, so kilograms keep one
     * decimal, while pounds round to 0.5 lb: a typed 100 lb comes back as 100.09 lb.
     */
    public static display(value: number, unit: WeightUnit): number {
        return WeightUtils.roundTo(
            value,
            unit == WeightUnit.Pounds ? 0.5 : 0.1,
        );
    }

    // Rounded to tenths too: multiples of 0.5 or 0.1 pick up float noise
    private static roundTo(value: number, step: number): number {
        return Math.round(Math.round(value / step) * step * 10) / 10;
    }
}
