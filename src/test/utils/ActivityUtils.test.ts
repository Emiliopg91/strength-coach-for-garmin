import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ActivityUtils, WorkoutLoad } from "@/utils/ActivityUtils";
import {
    BodyMetricListItem,
    ActivityDetails,
    ActivityListItem,
    ActivityStepDto,
    StepType,
} from "@/utils/backend/models";
import { KG_TO_LB } from "@/context/I18nSettingsContext";

const kgToKg = (kg: number) => kg;
const kgToLb = (kg: number) => kg * KG_TO_LB;

/** Degrees expressed in Garmin semicircles. */
const semicircles = (degrees: number) => (degrees * 2 ** 31) / 180;

const makeDetails = (
    overrides: Partial<ActivityDetails> = {},
): ActivityDetails => ({
    active_time: 0,
    coordinates: [],
    device: null,
    distance: null,
    heart_rates: [],
    max_hr: 189,
    laps: [],
    name: "Activity",
    notes: "",
    steps: [],
    speeds: [],
    sport: 0,
    sub_sport: 0,
    timestamp: 0,
    active_calories: 0,
    timer_time: 0,
    training_load: 0,
    primary_muscles: [],
    secondary_muscles: [],
    connect_id: null,
    feeling: null,
    rpe: null,
    ...overrides,
});

const makeSet = (
    overrides: Partial<ActivityStepDto> = {},
): ActivityStepDto => ({
    ex_cat: 0,
    ex_id: 0,
    idx: 0,
    pr: false,
    reps: 10,
    reps_ref: null,
    kind: StepType.Exercise,
    weight: 50,
    weight_ref: null,
    duration: 0,
    duration_ref: null,
    ...overrides,
});

describe("ActivityUtils.detailsFromBackend - sets", () => {
    it("keeps rest steps without reps nor weight out of the volume", () => {
        const details = ActivityUtils.detailsFromBackend(
            makeDetails({
                steps: [
                    makeSet({ idx: 0, reps: 10, weight: 50 }),
                    makeSet({
                        idx: 1,
                        ex_cat: null,
                        ex_id: null,
                        kind: StepType.Rest,
                        reps: null,
                        weight: null,
                    }),
                ],
            }),
            (kg) => kg,
        );

        expect(details.volume).toBe(500);
        expect(details.steps[1].reps).toBeNull();
        expect(details.steps[1].weight).toBeNull();
    });

    it("computes volume across all sets", () => {
        const details = ActivityUtils.detailsFromBackend(
            makeDetails({
                steps: [
                    makeSet({
                        idx: 0,
                        ex_cat: 1,
                        ex_id: 2,
                        reps: 10,
                        weight: 60,
                    }),
                    makeSet({
                        idx: 1,
                        ex_cat: 3,
                        ex_id: 4,
                        reps: 8,
                        weight: 40,
                    }),
                    makeSet({
                        idx: 2,
                        ex_cat: 1,
                        ex_id: 2,
                        reps: 5,
                        weight: 70,
                    }),
                ],
            }),
            kgToKg,
        );

        expect(details.volume).toBe(10 * 60 + 8 * 40 + 5 * 70);
    });

    it("converts weights to the selected unit rounded to one decimal", () => {
        const details = ActivityUtils.detailsFromBackend(
            makeDetails({ steps: [makeSet({ reps: 2, weight: 100 })] }),
            kgToLb,
        );

        expect(details.steps[0].weight).toBe(220.5);
        expect(details.volume).toBeCloseTo(441, 6);
    });

    it("rounds kilograms to one decimal", () => {
        const details = ActivityUtils.detailsFromBackend(
            makeDetails({ steps: [makeSet({ weight: 62.46 })] }),
            kgToKg,
        );

        expect(details.steps[0].weight).toBe(62.5);
    });

    it("leaves defaults when there are no sets", () => {
        const details = ActivityUtils.detailsFromBackend(makeDetails(), kgToKg);

        expect(details.volume).toBe(0);
    });
});

describe("ActivityUtils.detailsFromBackend - laps", () => {
    it("converts lap start positions from semicircles to degrees", () => {
        const details = ActivityUtils.detailsFromBackend(
            makeDetails({
                laps: [
                    {
                        idx: 0,
                        start_latitude: semicircles(40),
                        start_longitude: semicircles(-3),
                    },
                    { idx: 1, start_latitude: null, start_longitude: null },
                ],
            }),
            kgToKg,
        );

        expect(details.laps[0].start_latitude).toBeCloseTo(40, 10);
        expect(details.laps[0].start_longitude).toBeCloseTo(-3, 10);
        expect(details.laps[1].start_latitude).toBeNull();
        expect(details.laps[1].start_longitude).toBeNull();
    });
});

describe("ActivityUtils.detailsFromBackend - GPS", () => {
    it("finds start/finish points and skips missing coordinates", () => {
        const details = ActivityUtils.detailsFromBackend(
            makeDetails({
                distance: 5,
                coordinates: [
                    null,
                    [semicircles(40), semicircles(-3)],
                    [semicircles(41), semicircles(-3)],
                    [semicircles(42), semicircles(-4)],
                    null,
                ],
            }),
            kgToKg,
        );

        expect(details.start_point[0]).toBeCloseTo(40, 10);
        expect(details.start_point[1]).toBeCloseTo(-3, 10);
        expect(details.finish_point[0]).toBeCloseTo(42, 10);
        expect(details.finish_point[1]).toBeCloseTo(-4, 10);
        expect(details.valid_points).toHaveLength(3);
    });

    it("keeps the distance reported by the device", () => {
        const details = ActivityUtils.detailsFromBackend(
            makeDetails({
                distance: 5,
                coordinates: [
                    [semicircles(0), semicircles(0)],
                    [semicircles(1), semicircles(0)],
                ],
            }),
            kgToKg,
        );

        expect(details.distance).toBe(5);
    });

    it("paints every segment blue when there is no speed data", () => {
        const details = ActivityUtils.detailsFromBackend(
            makeDetails({
                distance: 1,
                coordinates: [
                    [semicircles(0), semicircles(0)],
                    [semicircles(1), semicircles(0)],
                    [semicircles(2), semicircles(0)],
                ],
            }),
            kgToKg,
        );

        expect(details.gps_segments).toHaveLength(2);
        expect(details.gps_segments.map((s) => s.color)).toEqual([
            "hsl(240, 100%, 50%)",
            "hsl(240, 100%, 50%)",
        ]);
        expect(details.gps_segments[1].coordinates[0][0]).toBeCloseTo(1, 10);
        expect(details.gps_segments[1].coordinates[1][0]).toBeCloseTo(2, 10);
    });

    it("colors segments by speed percentile, from blue (slow) to red (fast)", () => {
        const details = ActivityUtils.detailsFromBackend(
            makeDetails({
                distance: 1,
                coordinates: [
                    [semicircles(0), semicircles(0)],
                    [semicircles(1), semicircles(0)],
                    [semicircles(2), semicircles(0)],
                    [semicircles(3), semicircles(0)],
                ],
                speeds: [1, 2, 3],
            }),
            kgToKg,
        );

        expect(details.gps_segments.map((s) => s.color)).toEqual([
            "hsl(200, 100%, 50%)",
            "hsl(120, 100%, 50%)",
            "hsl(40, 100%, 50%)",
        ]);
    });
});

describe("ActivityUtils.detailsFromBackend - heart rate", () => {
    it("distributes elapsed time across zones using a 189 bpm floor for max HR", () => {
        const details = ActivityUtils.detailsFromBackend(
            makeDetails({
                timer_time: 100,
                heart_rates: [100, 150, 180, null],
            }),
            kgToKg,
        );

        // 100 & missing -> Z1, 150 (79%) -> Z3, 180 (95%) -> Z5
        expect(details.zones_times).toEqual([50, 0, 25, 0, 25]);
    });

    it("uses the observed max HR when it is above 189", () => {
        const details = ActivityUtils.detailsFromBackend(
            makeDetails({ timer_time: 20, heart_rates: [200, 100] }),
            kgToKg,
        );

        expect(details.zones_times).toEqual([10, 0, 0, 0, 10]);
        expect(details.hrBreathData.map((d) => d.color)).toEqual([
            "red",
            "gray",
        ]);
    });

    it("adjusts the first zone so zones always add up to the elapsed time", () => {
        const details = ActivityUtils.detailsFromBackend(
            makeDetails({
                timer_time: 10,
                heart_rates: [100, 120, 150, 160],
            }),
            kgToKg,
        );

        // Each zone rounds 2.5 up to 3, so Z1 absorbs the -2 difference
        expect(details.zones_times).toEqual([1, 3, 3, 3, 0]);
        expect(details.zones_times.reduce((a, b) => a + b, 0)).toBe(10);
    });

    it("computes min/avg/max ignoring missing samples", () => {
        const details = ActivityUtils.detailsFromBackend(
            makeDetails({
                timer_time: 4,
                heart_rates: [100, null, 150, 180],
            }),
            kgToKg,
        );

        expect(details.hrRanges).toEqual([100, 143, 180]);
    });

    it.each([
        ["no samples", []],
        ["only missing samples", [null, null, null]],
    ])(
        "keeps HR defaults with %s",
        (_: unknown, heart_rates: (number | null)[]) => {
            const details = ActivityUtils.detailsFromBackend(
                makeDetails({ timer_time: 30, heart_rates }),
                kgToKg,
            );

            expect(details.zones_times).toEqual([0, 0, 0, 0, 0]);
            expect(details.hrRanges).toEqual([0, 0, 0]);
            expect(details.hrBreathData).toEqual([]);
        },
    );

    it("builds per-sample chart data colored by zone", () => {
        const details = ActivityUtils.detailsFromBackend(
            makeDetails({
                timer_time: 5,
                heart_rates: [100, 125, 150, 165, 180],
            }),
            kgToKg,
        );

        expect(details.hrBreathData).toEqual([
            { idx: 0, hr: 100, avg: 144, color: "gray" },
            { idx: 1, hr: 125, avg: 144, color: "turquoise" },
            { idx: 2, hr: 150, avg: 144, color: "green" },
            { idx: 3, hr: 165, avg: 144, color: "orange" },
            { idx: 4, hr: 180, avg: 144, color: "red" },
        ]);
    });
});

describe("ActivityUtils.detailsFromBackend - input", () => {
    const backDetails = () =>
        makeDetails({
            steps: [makeSet({ weight: 100 })],
            coordinates: [
                [semicircles(40), semicircles(-3)],
                [semicircles(41), semicircles(-3)],
            ],
            laps: [
                {
                    idx: 0,
                    start_latitude: semicircles(40),
                    start_longitude: semicircles(-3),
                },
            ],
            timer_time: 2,
            heart_rates: [120, 150],
        });

    it("does not modify the backend details", () => {
        const back = backDetails();
        const snapshot = structuredClone(back);

        ActivityUtils.detailsFromBackend(back, kgToLb);

        expect(back).toEqual(snapshot);
    });

    it("gives the same result when called twice with the same details", () => {
        const back = backDetails();

        const first = ActivityUtils.detailsFromBackend(back, kgToLb);
        const second = ActivityUtils.detailsFromBackend(back, kgToLb);

        expect(second).toEqual(first);
    });
});

describe("ActivityUtils.isOverreaching", () => {
    const load = (current: number, reference: number): WorkoutLoad => ({
        date: 0,
        upper: 0,
        lower: 0,
        current,
        reference,
    });

    it("is false without data", () => {
        expect(ActivityUtils.isOverreaching([])).toBe(false);
    });

    it("only looks at the last entry", () => {
        expect(
            ActivityUtils.isOverreaching([load(1000, 1), load(100, 100)]),
        ).toBe(false);
        expect(
            ActivityUtils.isOverreaching([load(100, 100), load(151, 100)]),
        ).toBe(true);
    });

    it("starts at the ACWR upper ratio, as Garmin's high load", () => {
        expect(ActivityUtils.isOverreaching([load(149, 100)])).toBe(false);
        expect(ActivityUtils.isOverreaching([load(150, 100)])).toBe(true);
    });
});

describe("ActivityUtils.loadZone", () => {
    it("rates the load against the optimal range, upper edge included as high", () => {
        expect(ActivityUtils.loadZone(79, 80, 150)).toBe("below");
        expect(ActivityUtils.loadZone(80, 80, 150)).toBe("optimal");
        expect(ActivityUtils.loadZone(149, 80, 150)).toBe("optimal");
        expect(ActivityUtils.loadZone(150, 80, 150)).toBe("above");
    });
});

describe("ActivityUtils.loadSegments", () => {
    // `upper` is the height of the band over `lower`, as the chart stacks them
    const point = (date: number, current: number): WorkoutLoad => ({
        date,
        current,
        lower: 80,
        upper: 70,
        reference: 100,
    });

    it("returns nothing for less than two points", () => {
        expect(ActivityUtils.loadSegments([])).toEqual([]);
        expect(ActivityUtils.loadSegments([point(0, 100)])).toEqual([]);
    });

    it("keeps a segment whole when it stays in one zone", () => {
        expect(
            ActivityUtils.loadSegments([point(0, 100), point(10, 120)]),
        ).toEqual([{ x0: 0, y0: 100, x1: 10, y1: 120, zone: "optimal" }]);
    });

    it("splits a segment where it crosses the upper edge", () => {
        expect(
            ActivityUtils.loadSegments([point(0, 130), point(10, 170)]),
        ).toEqual([
            { x0: 0, y0: 130, x1: 5, y1: 150, zone: "optimal" },
            { x0: 5, y0: 150, x1: 10, y1: 170, zone: "above" },
        ]);
    });

    it("splits a segment crossing the whole band into three", () => {
        const zones = ActivityUtils.loadSegments([
            point(0, 60),
            point(10, 160),
        ]).map((s) => [s.x0, s.x1, s.zone]);

        expect(zones).toEqual([
            [0, 2, "below"],
            [2, 9, "optimal"],
            [9, 10, "above"],
        ]);
    });

    it("follows a moving band", () => {
        // The line stays at 100 while the lower edge rises from 80 to 120: crossing at the middle
        const segments = ActivityUtils.loadSegments([
            { date: 0, current: 100, lower: 80, upper: 70, reference: 100 },
            { date: 10, current: 100, lower: 120, upper: 70, reference: 150 },
        ]);

        expect(segments.map((s) => [s.x0, s.x1, s.zone])).toEqual([
            [0, 5, "optimal"],
            [5, 10, "below"],
        ]);
    });
});

describe("ActivityUtils.calculateWorkoutLoad", () => {
    const DAY_MS = 24 * 60 * 60 * 1000;
    const NOW = new Date(2025, 5, 15, 18, 30);
    const TODAY = new Date(2025, 5, 15).getTime();

    const activity = (
        daysAgo: number,
        training_load: number,
    ): ActivityListItem => {
        const date = new Date(NOW);
        date.setDate(date.getDate() - daysAgo);
        return {
            active_calories: 0,
            has_record: false,
            name: "Activity",
            sport: 0,
            sub_sport: 0,
            timestamp: date.getTime() / 1000,
            timer_time: 0,
            training_load,
            has_sets: false,
        };
    };

    beforeEach(() => {
        vi.useFakeTimers();
        vi.setSystemTime(NOW);
    });

    afterEach(() => {
        vi.useRealTimers();
    });

    it("returns nothing without activities", () => {
        expect(ActivityUtils.calculateWorkoutLoad([])).toEqual([]);
    });

    it("returns one entry per day for the last chronic window, ending today", () => {
        const result = ActivityUtils.calculateWorkoutLoad([activity(0, 100)]);

        expect(result).toHaveLength(ActivityUtils.CHRONIC_DAYS);
        expect(result[result.length - 1].date).toBe(TODAY);
        for (let i = 1; i < result.length; i++) {
            // Allow for DST shifts
            expect(
                Math.abs(result[i].date - result[i - 1].date - DAY_MS),
            ).toBeLessThanOrEqual(60 * 60 * 1000);
        }
    });

    it("applies Garmin's acute and chronic EWMA to a single load spike", () => {
        const result = ActivityUtils.calculateWorkoutLoad([activity(0, 100)]);
        const last = result[result.length - 1];

        // acute: 7 x EWMA(N = 10) of the daily load; chronic: EWMA(N = 18) of the acute load
        const acute = (100 * 7 * 2) / 11;
        const chronic = (acute * 2) / 19;
        expect(last.current).toBeCloseTo(acute, 10);
        expect(last.reference).toBeCloseTo(chronic, 10);
        expect(last.lower).toBeCloseTo(chronic * 0.8, 10);
        expect(last.upper).toBeCloseTo(chronic * 0.7, 10);
        expect(
            result
                .slice(0, -1)
                .every((e) => e.current === 0 && e.reference === 0),
        ).toBe(true);
        expect(ActivityUtils.isOverreaching(result)).toBe(true);
    });

    it("sums activities on the same day", () => {
        const single = ActivityUtils.calculateWorkoutLoad([activity(0, 100)]);
        const split = ActivityUtils.calculateWorkoutLoad([
            activity(0, 60),
            activity(0, 40),
        ]);

        expect(split).toEqual(single);
    });

    it("converges to the weekly load with a constant daily load", () => {
        const activities = Array.from(
            { length: 2 * ActivityUtils.CHRONIC_DAYS },
            (_, i) => activity(i, 80),
        );
        const result = ActivityUtils.calculateWorkoutLoad(activities);
        const last = result[result.length - 1];

        // Both averages start from zero, so they get within 0.5% of 7 x 80 by the end
        expect(Math.abs(last.current - 560) / 560).toBeLessThan(0.005);
        expect(Math.abs(last.reference - 560) / 560).toBeLessThan(0.005);
        expect(ActivityUtils.isOverreaching(result)).toBe(false);
    });

    it("ignores activities older than twice the chronic window", () => {
        const result = ActivityUtils.calculateWorkoutLoad([
            activity(2 * ActivityUtils.CHRONIC_DAYS + 10, 500),
        ]);

        expect(result).toHaveLength(ActivityUtils.CHRONIC_DAYS);
        expect(result.every((e) => e.current === 0 && e.reference === 0)).toBe(
            true,
        );
    });
});

describe("ActivityUtils.maxE1rm", () => {
    it("returns 0 for no sets", () => {
        expect(ActivityUtils.maxE1rm([], false)).toBe(0);
    });

    it("returns the highest estimate across sets", () => {
        const sets = [
            makeSet({ weight: 100, reps: 1 }),
            makeSet({ weight: 90, reps: 5 }),
            makeSet({ weight: 60, reps: 12 }),
        ];

        expect(ActivityUtils.maxE1rm(sets, false)).toBe(
            Math.max(
                ActivityUtils.estimate1rm(100, 1, false),
                ActivityUtils.estimate1rm(90, 5, false),
                ActivityUtils.estimate1rm(60, 12, false),
            ),
        );
        expect(ActivityUtils.maxE1rm(sets, false)).toBe(101);
    });
});

describe("ActivityUtils.groupByExercises", () => {
    it("skips steps without exercise but keeps warm-ups with one", () => {
        const { exercises } = ActivityUtils.groupByExercises([
            makeSet({ kind: StepType.Warmup, ex_cat: null, ex_id: null }),
            makeSet({ kind: StepType.Warmup, ex_cat: 1, ex_id: 2 }),
            makeSet({ kind: StepType.Rest, ex_cat: null, ex_id: null }),
            makeSet({ ex_cat: 3, ex_id: 4 }),
        ]);

        expect(exercises).toEqual(["1-2", "3-4"]);
    });
});

describe("ActivityUtils.exerciseChartData", () => {
    const activities = {
        "Torso\n2000": {
            series: [
                makeSet({ weight: 60, reps: 8 }),
                makeSet({ weight: 50, reps: 12 }),
            ],
            weight: 80,
        },
        "Torso\n1000": {
            series: [makeSet({ weight: 55, reps: 10 })],
            weight: null,
        },
        "Torso\n3000": {
            series: [makeSet({ weight: 70, reps: 6 })],
            weight: 75,
        },
    };

    it("keeps every activity with sets, sorted by date", () => {
        const points = ActivityUtils.exerciseChartData(activities, false);

        expect(points.map((p) => p.date)).toEqual([1000, 2000, 3000]);
    });

    it("takes the heaviest weight, average reps, best e1RM and e1RM relative to bodyweight", () => {
        const point = ActivityUtils.exerciseChartData(activities, false).find(
            (p) => p.date === 2000,
        );

        expect(point).toEqual({
            date: 2000,
            weight: 60,
            reps: 10,
            e1rm: ActivityUtils.maxE1rm(
                activities["Torso\n2000"].series,
                false,
            ),
            relativeWeight:
                ActivityUtils.maxE1rm(activities["Torso\n2000"].series, false) /
                80,
        });
    });

    it("uses null as relative weight when the bodyweight is unknown", () => {
        const [point] = ActivityUtils.exerciseChartData(activities, false);

        expect(point.date).toBe(1000);
        expect(point.relativeWeight).toBeNull();
        expect(point.e1rm).toBe(ActivityUtils.estimate1rm(55, 10, false));
    });

    it("estimates the 1RM per side for unilateral exercises", () => {
        const [point] = ActivityUtils.exerciseChartData(
            {
                "Torso\n1000": {
                    series: [makeSet({ weight: 20, reps: 20 })],
                    weight: null,
                },
            },
            true,
        );

        expect(point.e1rm).toBe(ActivityUtils.estimate1rm(20, 20, true));
        expect(point.e1rm).toBe(ActivityUtils.estimate1rm(20, 10, false));
    });

    it("skips activities without sets", () => {
        expect(
            ActivityUtils.exerciseChartData(
                { "Torso\n1000": { series: [], weight: 80 } },
                false,
            ),
        ).toEqual([]);
    });
});

describe("ActivityUtils.bodyweightAt", () => {
    const metric = (date: number, weight: number): BodyMetricListItem => ({
        date,
        weight,
        fat_ratio: 0,
        muscle_mass: 0,
        water_ratio: 0,
    });
    const metrics = [metric(2000, 80), metric(1000, 82), metric(3000, 78)];

    it("takes the latest measurement before the date", () => {
        expect(ActivityUtils.bodyweightAt(metrics, 2500)).toBe(80);
        expect(ActivityUtils.bodyweightAt(metrics, 5000)).toBe(78);
    });

    it("ignores a measurement on the same date", () => {
        expect(ActivityUtils.bodyweightAt(metrics, 2000)).toBe(82);
    });

    it("falls back to the latest measurement when none is before", () => {
        expect(ActivityUtils.bodyweightAt(metrics, 500)).toBe(78);
    });

    it("returns null without measurements", () => {
        expect(ActivityUtils.bodyweightAt([], 1000)).toBeNull();
    });
});
