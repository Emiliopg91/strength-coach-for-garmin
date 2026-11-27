import { useLoadingTask } from "@/hooks/useLoadingTask";
import { I18nSettingsContext } from "@/context/I18nSettingsContext";
import { BackendClient } from "@/utils/backend/client";
import { useContext, useState } from "react";
import {
    Button,
    Dialog,
    DialogContent,
    DialogTitle,
    IconButton,
    TextareaAutosize,
    Tooltip,
} from "@mui/material";
import CloseIcon from "@mui/icons-material/Close";
import DownloadIcon from "@mui/icons-material/Download";
import { ActivityFrontDetails } from "@/utils/ActivityUtils";
import { ActivityMap } from "../charts/ActivityMap";
import { ActivityDetailsTable } from "../tables/ActivityDetailsTable";
import { ActivityHeartRateChart } from "../charts/ActivityHeartRateChart";
import { ActivitySetsTable } from "../tables/ActivitySetsTable";
import "@/styles/Activities/ActivityModal.css";
import { MuscleModel } from "../../Exercises/charts/MuscleModel";
import { ActivityUpdate } from "@/utils/backend/models";
import { closeUnlessBackdrop } from "@/utils/DialogUtils";

type Props = {
    activity: ActivityFrontDetails;
    onClose: () => void;
    onUpdate: () => void;
};

export function ActivityModal({ activity, onClose, onUpdate }: Props) {
    const withLoading = useLoadingTask();
    const { translate, toKg } = useContext(I18nSettingsContext);
    const [originalActivity] = useState(activity);
    const [localActivity, setLocalActivity] = useState({ ...activity });
    const [notes, setNotes] = useState(activity.notes);

    const changed =
        notes != originalActivity.notes ||
        localActivity.steps.some(
            (step, idx) =>
                step.reps != originalActivity.steps[idx].reps ||
                step.weight != originalActivity.steps[idx].weight ||
                step.ex_cat != originalActivity.steps[idx].ex_cat ||
                step.ex_id != originalActivity.steps[idx].ex_id,
        );

    const updateSerieExercise = (
        idx: number,
        newCat: number,
        newId: number,
    ) => {
        setLocalActivity((prev) => {
            const steps = prev.steps.map((s, i) =>
                i == idx ? { ...s, ex_cat: newCat, ex_id: newId } : s,
            );

            return { ...prev, steps };
        });
    };

    const updateSerieRepsWeight = (
        idx: number,
        field: "reps" | "weight",
        value: number,
    ) => {
        setLocalActivity((prev) => {
            const steps = prev.steps.slice();
            steps[idx] = { ...steps[idx], [field]: value };

            return {
                ...prev,
                steps,
            };
        });
    };

    const updateNote = (notes: string) => {
        setNotes(notes);
        setLocalActivity((prev) => ({ ...prev, notes }));
    };

    const saveChanges = () => {
        const update: ActivityUpdate = {
            timestamp: localActivity.timestamp,
            sets: [],
            notes: notes.length > 0 ? notes : null,
        };
        localActivity.steps.forEach((steps, serIdx) => {
            if (
                originalActivity.steps[serIdx].reps != steps.reps ||
                originalActivity.steps[serIdx].weight != steps.weight ||
                originalActivity.steps[serIdx].ex_cat != steps.ex_cat ||
                originalActivity.steps[serIdx].ex_id != steps.ex_id
            ) {
                update.sets.push({
                    ...steps,
                    weight: steps.weight === null ? null : toKg(steps.weight),
                });
            }
        });
        withLoading(BackendClient.saveActivityChanges(update)).then(() => {
            onUpdate();
            onClose();
        });
    };

    const downloadActivity = () => {
        withLoading(BackendClient.downloadActivity(localActivity.timestamp));
    };

    return (
        <Dialog
            open={true}
            onClose={closeUnlessBackdrop(onClose)}
            fullWidth
            maxWidth="sm"
        >
            <DialogTitle>
                {localActivity.name.length > 0 && (
                    <span>{localActivity.name}</span>
                )}
                {localActivity.name.length == 0 && (
                    <span>
                        {translate("sport_" + localActivity.sport) +
                            " - " +
                            translate(
                                "subsport_" +
                                    localActivity.sport +
                                    "_" +
                                    localActivity.sub_sport,
                            )}
                    </span>
                )}
                <Tooltip title={translate("download_activity")}>
                    {/* A disabled button fires no events, the tooltip needs this wrapper */}
                    <span className="activity-download-button">
                        <IconButton
                            aria-label={translate("download_activity")}
                            disabled={changed}
                            onClick={downloadActivity}
                        >
                            <DownloadIcon />
                        </IconButton>
                    </span>
                </Tooltip>
                <IconButton onClick={onClose} className="modal-close-button">
                    <CloseIcon />
                </IconButton>
            </DialogTitle>

            <DialogContent dividers>
                {localActivity.valid_points.length > 0 && (
                    <>
                        <ActivityMap
                            validPoints={localActivity.valid_points}
                            gpsSegments={localActivity.gps_segments}
                            startPoint={localActivity.start_point}
                            finishPoint={localActivity.finish_point}
                            laps={localActivity.laps}
                        />
                        <hr />
                    </>
                )}
                <div className="activity-details-row">
                    <ActivityDetailsTable activity={localActivity} />
                    {(localActivity.primary_muscles.length > 0 ||
                        localActivity.secondary_muscles.length > 0) && (
                        <div className="activity-muscle-model">
                            <MuscleModel
                                primary={localActivity.primary_muscles}
                                secondary={localActivity.secondary_muscles}
                            />
                        </div>
                    )}
                </div>
                {localActivity.hrBreathData.length > 0 && (
                    <>
                        <hr />
                        <ActivityHeartRateChart
                            hrBreathData={localActivity.hrBreathData}
                            hrRanges={localActivity.hrRanges}
                            zonesTimes={localActivity.zones_times}
                            totalElapsedTime={localActivity.total_elapsed_time}
                        />
                        <br />
                    </>
                )}
                {localActivity.steps &&
                    Object.keys(localActivity.steps).length > 0 && (
                        <ActivitySetsTable
                            series={localActivity.steps}
                            onUpdateSerieRepsWeight={updateSerieRepsWeight}
                            onUpdateSerieExercise={updateSerieExercise}
                        />
                    )}
                <div>
                    <TextareaAutosize
                        minRows={4}
                        placeholder={translate("activity_notes")}
                        value={notes}
                        style={{
                            width: "100%",
                            backgroundColor: "#202020",
                            color: "white",
                            resize: "none",
                        }}
                        onChange={(e) => {
                            updateNote(e.target.value);
                        }}
                    />
                </div>
                <div className="activity-sets-actions">
                    <Button
                        id="import-button"
                        variant="contained"
                        disabled={!changed}
                        className="full-width-button"
                        onClick={saveChanges}
                    >
                        {translate("update_activity")}
                    </Button>
                </div>
            </DialogContent>
        </Dialog>
    );
}
