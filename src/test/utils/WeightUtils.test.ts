import { describe, expect, it } from "vitest";
import { WeightUnit } from "@/utils/backend/models";
import { WeightUtils } from "@/utils/WeightUtils";

describe("WeightUtils", () => {
    it("steps by 0.5 kg and 1 lb", () => {
        expect(WeightUtils.step(WeightUnit.Kilograms)).toBe(0.5);
        expect(WeightUtils.step(WeightUnit.Pounds)).toBe(1);
    });

    it("snaps kilograms to the nearest 0.5", () => {
        expect(WeightUtils.snap(12.3, WeightUnit.Kilograms)).toBe(12.5);
        expect(WeightUtils.snap(12.2, WeightUnit.Kilograms)).toBe(12);
        expect(WeightUtils.snap(12.75, WeightUnit.Kilograms)).toBe(13);
        expect(WeightUtils.snap(0.1, WeightUnit.Kilograms)).toBe(0);
    });

    it("snaps pounds to the nearest whole pound", () => {
        expect(WeightUtils.snap(100.4, WeightUnit.Pounds)).toBe(100);
        expect(WeightUtils.snap(100.5, WeightUnit.Pounds)).toBe(101);
    });

    it("displays kilograms to 0.1 and pounds to 0.5", () => {
        expect(WeightUtils.display(12.1875, WeightUnit.Kilograms)).toBe(12.2);
        // 100 lb stored as 45.4 kg reads back as 100.09 lb
        expect(WeightUtils.display(100.09, WeightUnit.Pounds)).toBe(100);
        expect(WeightUtils.display(100.6, WeightUnit.Pounds)).toBe(100.5);
    });
});
