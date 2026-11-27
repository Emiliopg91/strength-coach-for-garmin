import { describe, expect, it, vi } from "vitest";
import { closeUnlessBackdrop } from "@/utils/DialogUtils";

describe("closeUnlessBackdrop", () => {
    it("ignores clicks outside the dialog", () => {
        const onClose = vi.fn();
        closeUnlessBackdrop(onClose)({}, "backdropClick");
        expect(onClose).not.toHaveBeenCalled();
    });

    it("closes on escape", () => {
        const onClose = vi.fn();
        closeUnlessBackdrop(onClose)({}, "escapeKeyDown");
        expect(onClose).toHaveBeenCalledOnce();
    });
});
