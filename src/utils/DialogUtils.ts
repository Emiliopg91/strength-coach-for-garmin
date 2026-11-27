/** Why a MUI dialog asks to close. */
export type DialogCloseReason = "backdropClick" | "escapeKeyDown";

/**
 * `onClose` handler for a MUI `Dialog` that runs `onClose` unless the click was outside the dialog,
 * so a stray click doesn't throw away what the dialog shows. Escape still closes it.
 */
export function closeUnlessBackdrop(onClose: () => void) {
    return (_event: object, reason: DialogCloseReason) => {
        if (reason !== "backdropClick") {
            onClose();
        }
    };
}
