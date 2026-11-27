import { AppContext } from "@/context/AppContext";
import { JSX, useContext } from "react";
import PageContent from "../layout/PageContent";
import { I18nSettingsContext } from "@/context/I18nSettingsContext";
import { ExerciseDetails, ActivityDetails } from "@/utils/backend/models";
import NoPageBreak from "../layout/NoPageBreak";
import { ActivityUtils } from "@/utils/ActivityUtils";

type Props = {
    activities: ActivityDetails[];
    exercises: ExerciseDetails[];
};

export default function ActivitiesSection({
    activities,
    exercises: exercises_details,
}: Props): JSX.Element {
    const {
        translate,
        formatTimeDate,
        formatDuration,
        fromKg,
        getWeightUnit,
        formatNumber,
    } = useContext(I18nSettingsContext);
    const { exerciseName } = useContext(AppContext);

    return (
        <PageContent>
            <h2>{translate("activities")}</h2>
            <div className="section-container">
                {activities.map((activity) => {
                    const { exercises, groupedSeries } =
                        ActivityUtils.groupByExercises(activity.steps);
                    return (
                        <NoPageBreak key={activity.timestamp}>
                            <h4>
                                {activity.name +
                                    " @ " +
                                    formatTimeDate(activity.timestamp)}
                            </h4>
                            <div className="section-container">
                                <div className="report-table">
                                    <b>{translate("sport") + ": "}</b>
                                    {translate("sport_" + activity.sport) +
                                        " - " +
                                        translate(
                                            "subsport_" +
                                                activity.sport +
                                                "_" +
                                                activity.sub_sport,
                                        )}
                                    <br />
                                    <b>{translate("time") + ": "}</b>
                                    {formatDuration(activity.active_time) +
                                        " / " +
                                        formatDuration(activity.timer_time)}
                                    <br />
                                    <b>{translate("active_calories") + ": "}</b>
                                    {activity.active_calories}
                                    <br />
                                    <b>{translate("workout_load") + ": "}</b>
                                    {formatNumber(activity.training_load, 0)}
                                </div>
                                <br />
                                <div className="report-table">
                                    <table>
                                        <thead>
                                            <tr>
                                                <th>{translate("exercise")}</th>
                                                <th>
                                                    {translate("repetitions")}
                                                </th>
                                                <th>{translate("weight")}</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {exercises.map((exercise) => {
                                                let name = exerciseName(
                                                    ...(exercise
                                                        .split("-")
                                                        .map(Number) as [
                                                        number,
                                                        number,
                                                    ]),
                                                );
                                                if (
                                                    exercises_details.find(
                                                        (e) =>
                                                            e.category +
                                                                "-" +
                                                                e.id ==
                                                            exercise,
                                                    )?.unilateral
                                                ) {
                                                    name =
                                                        name +
                                                        " (" +
                                                        translate(
                                                            "unilateral",
                                                        ) +
                                                        ")";
                                                }

                                                return (
                                                    <>
                                                        {groupedSeries[
                                                            exercise
                                                        ].map(
                                                            ([, set], idx) => (
                                                                <tr
                                                                    key={
                                                                        exercise
                                                                    }
                                                                >
                                                                    {idx ===
                                                                        0 && (
                                                                        <td
                                                                            rowSpan={
                                                                                groupedSeries[
                                                                                    exercise
                                                                                ]
                                                                                    .length
                                                                            }
                                                                        >
                                                                            {
                                                                                name
                                                                            }
                                                                        </td>
                                                                    )}
                                                                    <td>
                                                                        {
                                                                            set.reps
                                                                        }
                                                                    </td>
                                                                    <td>
                                                                        {fromKg(
                                                                            set.weight ??
                                                                                0,
                                                                        ) +
                                                                            " " +
                                                                            getWeightUnit()}
                                                                    </td>
                                                                </tr>
                                                            ),
                                                        )}
                                                    </>
                                                );
                                            })}
                                        </tbody>
                                    </table>
                                </div>
                            </div>
                        </NoPageBreak>
                    );
                })}
            </div>
        </PageContent>
    );
}
