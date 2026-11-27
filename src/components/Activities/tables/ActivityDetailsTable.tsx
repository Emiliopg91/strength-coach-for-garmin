import { I18nSettingsContext } from "@/context/I18nSettingsContext";
import { useLoadingTask } from "@/hooks/useLoadingTask";
import { ActivityFrontDetails } from "@/utils/ActivityUtils";
import { BackendClient } from "@/utils/backend/client";
import { FormControl, MenuItem, Select } from "@mui/material";
import { useContext, useState } from "react";

type Props = {
    activity: ActivityFrontDetails;
};

export function ActivityDetailsTable({ activity }: Props) {
    const withLoading = useLoadingTask();
    const {
        translate,
        toKm,
        fromKm,
        fromKg,
        getDistanceUnit,
        getWeightUnit,
        formatDuration,
        formatTimeDate,
        formatNumber,
    } = useContext(I18nSettingsContext);
    const [rpe, setRpe] = useState(activity.rpe);
    const [feeling, setFeeling] = useState(activity.feeling);

    const updateRpe = (value: number) => {
        withLoading(
            BackendClient.setActivityRpe(activity.timestamp, value),
        ).then(() => setRpe(value));
    };

    const updateFeeling = (value: number) => {
        withLoading(
            BackendClient.setActivityFeeling(activity.timestamp, value),
        ).then(() => setFeeling(value));
    };

    return (
        <table id="activity-details-table">
            <colgroup>
                <col className="col-200" />
                <col className="col-200" />
                <col />
            </colgroup>
            <tbody>
                <tr>
                    <td>{translate("date")}:</td>
                    <td>{formatTimeDate(activity.timestamp)}</td>
                </tr>
                <tr>
                    <td>{translate("time")}:</td>
                    <td>
                        {(activity.active_time > 0
                            ? formatDuration(activity.active_time) + " / "
                            : "") + formatDuration(activity.timer_time)}
                    </td>
                </tr>
                <tr>
                    <td>{translate("calories")}:</td>
                    <td>{formatNumber(activity.active_calories, 0)} Kcal</td>
                </tr>
                {activity.distance != null && activity.distance > 0 && (
                    <>
                        <tr>
                            <td>{translate("distance")}:</td>
                            <td>
                                {formatNumber(fromKm(activity.distance), 2)}{" "}
                                {getDistanceUnit()}
                            </td>
                        </tr>
                        <tr>
                            <td>{translate("speed")}:</td>
                            <td>
                                {formatNumber(
                                    fromKm(
                                        activity.distance /
                                            (activity.timer_time / 3600),
                                    ),
                                    2,
                                )}{" "}
                                {getDistanceUnit()}/h (
                                {formatDuration(
                                    toKm(
                                        activity.timer_time / activity.distance,
                                    ),
                                )}{" "}
                                min/{getDistanceUnit()}){" "}
                            </td>
                        </tr>
                    </>
                )}
                {activity.volume > 0 && (
                    <tr>
                        <td>{translate("volume")}:</td>
                        <td>
                            {formatNumber(fromKg(activity.volume), 1)}{" "}
                            {getWeightUnit()}
                        </td>
                    </tr>
                )}
                <tr>
                    <td>{translate("workout_load")}:</td>
                    <td>{formatNumber(activity.training_load, 0)}</td>
                </tr>
                <tr className="activity-details-select-row">
                    <td>{translate("rpe")}:</td>
                    <td>
                        <FormControl fullWidth size="small">
                            <Select
                                labelId="activity-rpe-label"
                                id="activity-rpe"
                                value={rpe ?? ""}
                                onChange={(e) =>
                                    updateRpe(Number(e.target.value))
                                }
                            >
                                {Array.from(
                                    { length: 10 },
                                    (_, i) => i + 1,
                                ).map((rpe) => (
                                    <MenuItem key={rpe} value={rpe}>
                                        {rpe + " - " + translate("rpe_" + rpe)}
                                    </MenuItem>
                                ))}
                            </Select>
                        </FormControl>
                    </td>
                </tr>
                <tr className="activity-details-select-row">
                    <td>{translate("feeling")}:</td>
                    <td>
                        <FormControl fullWidth size="small">
                            <Select
                                labelId="activity-feeling-label"
                                id="activity-feeling"
                                value={feeling ?? ""}
                                onChange={(e) =>
                                    updateFeeling(Number(e.target.value))
                                }
                            >
                                {Array.from({ length: 5 }, (_, i) => i).map(
                                    (feel) => (
                                        <MenuItem key={feel} value={feel}>
                                            {feel +
                                                " - " +
                                                translate("feeling_" + feel)}
                                        </MenuItem>
                                    ),
                                )}
                            </Select>
                        </FormControl>
                    </td>
                </tr>
                {activity.device && (
                    <tr>
                        <td>{translate("imported_from")}:</td>
                        <td>{activity.device}</td>
                    </tr>
                )}
            </tbody>
        </table>
    );
}
