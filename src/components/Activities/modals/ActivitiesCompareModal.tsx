import { AppContext } from "@/context/AppContext";
import { I18nSettingsContext } from "@/context/I18nSettingsContext";
import { Fragment, useContext } from "react";
import { Dialog, DialogContent, DialogTitle, IconButton } from "@mui/material";
import CloseIcon from "@mui/icons-material/Close";
import { ActivityFrontDetails } from "@/utils/ActivityUtils";
import { ActivityStepDto } from "@/utils/backend/models";

type Props = {
    activities: ActivityFrontDetails[];
    onClose: () => void;
};

export function ActivitiesCompareModal({ activities, onClose }: Props) {
    const {
        translate,
        fromKg,
        getWeightUnit,
        formatDuration,
        formatTimeDate,
        formatNumber,
    } = useContext(I18nSettingsContext);
    const { exerciseName } = useContext(AppContext);
    // Sets with exercise grouped by exercise ("cat-id"); exercises follow the order
    // of their first set across activities
    const exercises: string[] = [];
    const groupedSets = activities.map((s) => {
        const groups: Record<string, ActivityStepDto[]> = {};
        s.steps
            .filter((step) => step.ex_cat != null)
            .sort((a, b) => a.idx - b.idx)
            .forEach((step) => {
                const key = step.ex_cat + "-" + step.ex_id;
                if (!groups[key]) {
                    groups[key] = [];
                }
                groups[key].push(step);
            });
        return groups;
    });

    let pos = 0;
    while (true) {
        let any = false;
        for (const groups of groupedSets) {
            const keys = Object.keys(groups);
            if (keys.length > pos) {
                if (!exercises.includes(keys[pos])) {
                    exercises.push(keys[pos]);
                }
                any = true;
            }
        }
        if (!any) break;
        pos++;
    }

    return (
        <Dialog open={true} onClose={onClose}>
            <DialogTitle>
                {translate("activities_comparaison") +
                    ": " +
                    activities[0].name}
                <IconButton onClick={onClose} className="modal-close-button">
                    <CloseIcon />
                </IconButton>
            </DialogTitle>

            <DialogContent dividers>
                <table id="activities-compare-table">
                    <colgroup>
                        <col className="col-230" />
                        <col className="col-180" />
                        <col className="col-180" />
                    </colgroup>

                    <tbody>
                        <tr>
                            <td
                                style={{ borderBottom: "1px solid white" }}
                            ></td>
                            <td style={{ borderBottom: "1px solid white" }}>
                                <b>{formatTimeDate(activities[0].timestamp)}</b>
                            </td>
                            <td style={{ borderBottom: "1px solid white" }}>
                                <b>{formatTimeDate(activities[1].timestamp)}</b>
                            </td>
                        </tr>
                        <tr>
                            <td>{translate("time")}</td>
                            <td>
                                {formatDuration(activities[0].active_time) +
                                    " / " +
                                    formatDuration(
                                        activities[0].total_elapsed_time,
                                    )}
                            </td>
                            <td>
                                {formatDuration(activities[1].active_time) +
                                    " / " +
                                    formatDuration(
                                        activities[1].total_elapsed_time,
                                    )}
                            </td>
                        </tr>
                        <tr>
                            <td>{translate("calories")}</td>
                            <td>
                                {formatNumber(activities[0].active_calories, 0)}{" "}
                                Kcal
                            </td>
                            <td>
                                {formatNumber(activities[1].active_calories, 0)}{" "}
                                Kcal
                            </td>
                        </tr>
                        <tr>
                            <td>{translate("volume")}</td>
                            <td>
                                {formatNumber(fromKg(activities[0].volume), 1)}{" "}
                                {getWeightUnit()}
                            </td>
                            <td>
                                {formatNumber(fromKg(activities[1].volume), 1)}{" "}
                                {getWeightUnit()}
                            </td>
                        </tr>
                        <tr>
                            <td>{translate("workout_load")}</td>
                            <td>
                                {formatNumber(activities[0].training_load, 0)}
                            </td>
                            <td>
                                {formatNumber(activities[1].training_load, 0)}
                            </td>
                        </tr>
                        <tr
                            style={{
                                textAlign: "center",
                                paddingTop: "20px",
                                fontWeight: "bolder",
                            }}
                        >
                            <td style={{ borderBottom: "1px solid white" }}>
                                <br />
                                {translate("exercise")}
                            </td>
                            <td
                                style={{ borderBottom: "1px solid white" }}
                                colSpan={2}
                            >
                                <br />
                                {translate("series")}
                            </td>
                        </tr>
                        {exercises.map((exercise) => (
                            <tr
                                key={`exercise-${exercise}`}
                                style={{ textAlign: "center" }}
                            >
                                <td className="divider-bottom-white">
                                    {exerciseName(
                                        ...(exercise.split("-").map(Number) as [
                                            number,
                                            number,
                                        ]),
                                    )}
                                </td>
                                {groupedSets.map((groups, idx) => (
                                    <td
                                        key={`activity-${idx}`}
                                        className="divider-bottom-white"
                                    >
                                        {groups[exercise]?.map((set) => (
                                            <Fragment
                                                key={`activity-${idx}-set-${set.idx}`}
                                            >
                                                {set.reps +
                                                    " x " +
                                                    formatNumber(
                                                        set.weight ?? 0,
                                                        1,
                                                    ) +
                                                    " " +
                                                    getWeightUnit()}
                                                <br />
                                            </Fragment>
                                        ))}
                                    </td>
                                ))}
                            </tr>
                        ))}
                    </tbody>
                </table>
            </DialogContent>
        </Dialog>
    );
}
