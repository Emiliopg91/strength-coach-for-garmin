import { I18nSettingsContext } from "@/context/I18nSettingsContext";
import { ActivityFrontDetails } from "@/utils/ActivityUtils";
import { useContext } from "react";

type Props = {
    activity: ActivityFrontDetails;
};

export function ActivityDetailsTable({ activity }: Props) {
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
                            : "") + formatDuration(activity.total_elapsed_time)}
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
                                            (activity.total_elapsed_time /
                                                3600),
                                    ),
                                    2,
                                )}{" "}
                                {getDistanceUnit()}/h (
                                {formatDuration(
                                    toKm(
                                        activity.total_elapsed_time /
                                            activity.distance,
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
