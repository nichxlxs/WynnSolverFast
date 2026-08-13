"use strict";

const fs = require("fs");
const path = require("path");
const {
    createSandbox,
    loadGameData,
    decodeSolverUrl,
    decodeActiveNodes,
} = require("../../js/solver/tests/harness");

const root = path.resolve(__dirname, "../..");
const databasePath = path.join(__dirname, "functional-builds.json");
const itemPath = path.join(root, "data", "2.2.2.0", "items.json");
const majorIdPath = path.join(root, "data", "2.2.2.0", "majid.json");
const elementByRequirement = {
    strReq: ["E", "earth"],
    dexReq: ["T", "thunder"],
    intReq: ["W", "water"],
    defReq: ["F", "fire"],
    agiReq: ["A", "air"],
};

function serializableAspect(value) {
    if (!value) return null;
    const [aspect, tier] = value;
    if (!aspect) return null;
    return {
        name: aspect.display_name || aspect.displayName || aspect.name || null,
        internal_name: aspect.internal_name || aspect.internalName || null,
        tier,
    };
}

function overrideCurrentEncodingData(ctx) {
    const versionRoot = path.join(root, "data", "2.2.2.0");
    ctx.DEC = JSON.parse(fs.readFileSync(path.join(versionRoot, "encoding_consts.json"), "utf8"));
    ctx.ENC = ctx.DEC;
    ctx.ATREES = JSON.parse(fs.readFileSync(path.join(versionRoot, "atree.json"), "utf8"));
    ctx.MAJOR_IDS = JSON.parse(fs.readFileSync(path.join(versionRoot, "majid.json"), "utf8"));
    ctx.tomeRedirectMap = new Map();

    const aspects = JSON.parse(fs.readFileSync(path.join(versionRoot, "aspects.json"), "utf8"));
    const aspectMap = new Map();
    const aspectIdMap = new Map();
    for (const [playerClass, classAspects] of Object.entries(aspects)) {
        const byName = new Map();
        const byId = new Map();
        for (const aspect of classAspects) {
            byName.set(aspect.displayName, aspect);
            byId.set(aspect.id, aspect);
        }
        aspectMap.set(playerClass, byName);
        aspectIdMap.set(playerClass, byId);
    }
    ctx.aspect_map = aspectMap;
    ctx.aspectMap = aspectMap;
    ctx.aspect_id_map = aspectIdMap;
}

function main() {
    const payload = JSON.parse(fs.readFileSync(databasePath, "utf8"));
    const itemPayload = JSON.parse(fs.readFileSync(itemPath, "utf8"));
    const majorIds = JSON.parse(fs.readFileSync(majorIdPath, "utf8"));
    const itemMap = new Map(itemPayload.items.map((item) => [item.name, item]));
    const ctx = createSandbox();
    loadGameData(ctx);
    overrideCurrentEncodingData(ctx);

    let decodedCount = 0;
    const failures = [];
    for (const build of payload.builds) {
        try {
            const url = new URL(build.builder_url);
            const decoded = decodeSolverUrl(ctx, url.hash);
            const activeNodes = decoded.playerClass
                ? decodeActiveNodes(ctx, decoded.playerClass, decoded.atree_data)
                : [];
            const equipmentDetails = decoded.equipment.map((name) => itemMap.get(name)).filter(Boolean);
            const requirementMax = {};
            const requirementItemCount = {};
            for (const [key, [, element]] of Object.entries(elementByRequirement)) {
                const values = equipmentDetails.map((item) => item[key] || 0);
                requirementMax[element] = Math.max(0, ...values);
                requirementItemCount[element] = values.filter((value) => value > 0).length;
            }
            const elementCode = Object.entries(elementByRequirement)
                .filter(([key]) => equipmentDetails.some((item) => (item[key] || 0) > 0))
                .map(([, [letter]]) => letter)
                .join("");
            const equippedMajorIds = [];
            for (const item of equipmentDetails) {
                for (const majorId of item.majorIds || []) {
                    equippedMajorIds.push({
                        item: item.displayName,
                        internal_name: majorId,
                        name: majorIds[majorId]?.displayName || majorId,
                        description: majorIds[majorId]?.description || null,
                    });
                }
            }
            const archetypeCounts = {};
            for (const node of activeNodes) {
                const archetype = node.archetype || node.archetype_name;
                if (archetype) archetypeCounts[archetype] = (archetypeCounts[archetype] || 0) + 1;
            }
            build.decoded = {
                encoding_version_id: decoded.versionId,
                player_class: decoded.playerClass,
                level: decoded.level,
                equipment: decoded.equipment,
                powders: decoded.powders,
                tomes: decoded.tomes,
                assigned_skill_points: decoded.skillpoints,
                aspects: (decoded.aspects || []).map(serializableAspect).filter(Boolean),
                ability_nodes: activeNodes.map((node) => ({
                    id: node.id,
                    name: node.display_name || node.displayName || node.name,
                    archetype: node.archetype || node.archetype_name || null,
                })),
                archetype_node_counts: archetypeCounts,
                equipment_requirement_profile: {
                    code: elementCode || "N",
                    maximum_by_element: requirementMax,
                    item_count_by_element: requirementItemCount,
                },
                equipped_major_ids: equippedMajorIds,
            };
            decodedCount++;
        } catch (error) {
            failures.push({ id: build.id, error: String(error) });
            build.decoded = null;
        }
    }
    payload.decode = {
        decoded_records: decodedCount,
        failed_records: failures.length,
        failures,
        source_version: "data/2.2.2.0",
        interpretation: "Equipment requirement code uses ETWFA order and records which elemental skill requirements appear across the equipped items. It is a build-composition shorthand, not the weapon's damage-element list.",
    };
    fs.writeFileSync(databasePath, `${JSON.stringify(payload, null, 2)}\n`, "utf8");
    console.log(JSON.stringify(payload.decode, null, 2));
}

main();
