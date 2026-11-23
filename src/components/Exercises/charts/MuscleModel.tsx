import { JSX, MouseEvent } from "react";
import Body, { ExtendedBodyPart, Slug } from "@mjcdev/react-body-highlighter";

type Props = {
    primary: string[];
    secondary: string[];
    /// Each model is 200 * scale px wide and 400 * scale px tall
    scale?: number;
    /// Muscles of the region under the pointer, undefined when it leaves them. Makes the model
    /// react to the pointer
    onHover?: (muscles: string[] | undefined, event: MouseEvent) => void;
};

export function MuscleModel({
    primary: primary_input,
    secondary: secondary_input,
    scale = SCALE,
    onHover,
}: Props): JSX.Element {
    const intensities = new Map<Slug, number>();
    for (const slug of exercisesToSlugs(secondary_input)) {
        intensities.set(slug, SECONDARY_INTENSITY);
    }
    // Set last so a muscle both primary and secondary shows as primary
    for (const slug of exercisesToSlugs(primary_input)) {
        intensities.set(slug, PRIMARY_INTENSITY);
    }
    const data: ExtendedBodyPart[] = [...intensities].map(
        ([slug, intensity]) => ({ slug, intensity }),
    );

    // The library has no prop to disable the pointer cursor and click handling of its paths, nor
    // hover events: its paths carry the region slug as id
    // Takes out the empty side margins of the model
    const style = {
        margin: `0 ${-SIDE_MARGIN * scale}px`,
        pointerEvents: onHover ? undefined : ("none" as const),
    };
    const onMouseMove = onHover
        ? (event: MouseEvent) => {
              const slug = (event.target as Element).closest("path")?.id;
              const muscles = slug ? SLUG_MUSCLES[slug] : undefined;
              onHover(muscles, event);
          }
        : undefined;
    const onMouseLeave = onHover
        ? (event: MouseEvent) => onHover(undefined, event)
        : undefined;

    return (
        <>
            {(["front", "back"] as const).map((side) => (
                <div
                    key={side}
                    style={style}
                    onMouseMove={onMouseMove}
                    onMouseLeave={onMouseLeave}
                >
                    <Body
                        data={data}
                        colors={COLORS}
                        scale={scale}
                        side={side}
                    />
                </div>
            ))}
        </>
    );
}

// Each model is 200 * SCALE px wide and keeps its 1:2 ratio
const SCALE = 0.575;
// Empty side margin of the model at scale 1: at the default scale, each model is left 100px wide
const SIDE_MARGIN = 12;
// Indexed by intensity - 1
const COLORS = ["yellow", "red"];
const SECONDARY_INTENSITY = 1;
const PRIMARY_INTENSITY = 2;

// The model has no abductors region: the glutes are the closest hip abductors it shows
const MUSCLE_IDENTIFIERS: Record<string, Slug[]> = {
    abductors: ["gluteal"],
    abs: ["abs"],
    adductors: ["adductors"],
    biceps: ["biceps"],
    calves: ["calves"],
    chest: ["chest"],
    forearm: ["forearm"],
    glutes: ["gluteal"],
    hamstrings: ["hamstring"],
    hips: ["gluteal", "adductors"],
    lats: ["upper-back"],
    lower_back: ["lower-back"],
    obliques: ["obliques"],
    quads: ["quadriceps"],
    shoulders: ["deltoids"],
    traps: ["trapezius"],
    triceps: ["triceps"],
};

// Catalog muscles shown by each region of the model
const SLUG_MUSCLES: Record<string, string[]> = {};
for (const [muscle, slugs] of Object.entries(MUSCLE_IDENTIFIERS)) {
    for (const slug of slugs) {
        (SLUG_MUSCLES[slug] ??= []).push(muscle);
    }
}

function exercisesToSlugs(exercises: string[]): Slug[] {
    return exercises.flatMap((muscle) => MUSCLE_IDENTIFIERS[muscle] ?? []);
}
