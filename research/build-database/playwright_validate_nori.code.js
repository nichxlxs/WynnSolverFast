async page => {
    await page.context().addInitScript(() => {
        window.__wynn_validation_confirm_messages = [];
        window.confirm = message => {
            window.__wynn_validation_confirm_messages.push(String(message));
            return String(message).includes('created in an older version of wynncraft')
                || String(message).includes("You're using an unofficial version of WynnBuilder");
        };
    });
    await page.goto('about:blank');
    const response = await page.context().request.post('https://nori.fish/api/build/search', {
        data: {
            keyword: '',
            class_types: ['warrior', 'mage', 'archer', 'assassin', 'shaman'],
        },
    });
    if (!response.ok()) throw new Error(`Nori API returned HTTP ${response.status()}`);
    const builds = await response.json();
    const results = [];
    let pageErrors = [];
    let consoleErrors = [];
    page.on('pageerror', error => pageErrors.push(error.message));
    page.on('console', message => {
        if (message.type() === 'error') consoleErrors.push(message.text());
    });

    for (let index = 0; index < builds.length; index++) {
        const source = builds[index];
        pageErrors = [];
        consoleErrors = [];
        const started = Date.now();

        const isWynnBuilder = /^https:\/\/wynnbuilder(?:-beta)?\.github\.io\/+builder(?:\/|\?|#|$)/i
            .test(source.link);
        if (!isWynnBuilder) {
            results.push({
                source_index: index,
                source_url: source.link,
                status: 'not_wynnbuilder_link',
                valid_current_builder: false,
                elapsed_ms: Date.now() - started,
            });
            continue;
        }

        let navigationError = null;
        try {
            await page.goto('about:blank');
            await page.goto(source.link, { waitUntil: 'domcontentloaded', timeout: 20000 });
            await page.waitForFunction(() => {
                const errorText = document.querySelector('#err-box')?.textContent?.trim();
                const weapon = [...document.querySelectorAll('input')]
                    .find(input => input.placeholder === 'No Weapon');
                return Boolean(errorText || weapon?.value || document.body.innerText.includes('Effective HP'));
            }, null, { timeout: 12000 });
            await page.waitForTimeout(350);
        } catch (error) {
            navigationError = error.message;
        }

        let state = null;
        try {
            state = await page.evaluate(() => {
                const inputs = [...document.querySelectorAll('input')];
                const weapon = inputs.find(input => input.placeholder === 'No Weapon')?.value || null;
                const errorText = document.querySelector('#err-box')?.textContent?.trim() || '';
                const atreeWarning = document.querySelector('#atree-warning')?.textContent?.trim() || '';
                const apCostText = document.querySelector('#active_AP_cost')?.textContent?.trim() || '';
                const apCapText = document.querySelector('#active_AP_cap')?.textContent?.trim() || '';
                const bodyText = document.body.innerText;
                return {
                    current_game_version: typeof wynn_version_names !== 'undefined'
                        && typeof WYNN_VERSION_LATEST !== 'undefined'
                        ? wynn_version_names[WYNN_VERSION_LATEST]
                        : null,
                    encoded_version_id: typeof wynn_version_id !== 'undefined' ? wynn_version_id : null,
                    weapon,
                    error_text: errorText,
                    atree_warning: atreeWarning,
                    ap_cost: /^\d+$/.test(apCostText) ? Number(apCostText) : null,
                    ap_cap: /^\d+$/.test(apCapText) ? Number(apCapText) : null,
                    has_effective_hp: bodyText.includes('Effective HP:'),
                    has_assigned_skillpoints: bodyText.includes('Assigned ') && bodyText.includes(' skillpoints.'),
                    confirm_messages: window.__wynn_validation_confirm_messages || [],
                };
            });
        } catch (error) {
            navigationError ||= error.message;
        }

        const confirmMessages = state?.confirm_messages || [];
        const fatalDialogs = confirmMessages.filter(message =>
            !message.includes('created in an older version of wynncraft')
            && !message.includes("You're using an unofficial version of WynnBuilder"),
        );
        const abilityTreeValid = Boolean(
            state
            && !state.atree_warning
            && Number.isFinite(state.ap_cost)
            && Number.isFinite(state.ap_cap)
            && state.ap_cost <= state.ap_cap
        );
        const validCurrentBuilder = Boolean(
            !navigationError
            && state
            && state.weapon
            && state.has_effective_hp
            && state.has_assigned_skillpoints
            && !state.error_text
            && fatalDialogs.length === 0
            && pageErrors.length === 0
            && abilityTreeValid
        );
        const normalizedExpectedWeapon = String(source.weapon || '').trim().toLowerCase();
        const normalizedActualWeapon = String(state?.weapon || '').trim().toLowerCase();

        results.push({
            source_index: index,
            source_url: source.link,
            final_url: page.url(),
            current_game_version: state?.current_game_version || null,
            encoded_version_id: state?.encoded_version_id ?? null,
            status: validCurrentBuilder ? 'valid_current_builder' : 'broken_current_builder',
            valid_current_builder: validCurrentBuilder,
            version_update_prompted: confirmMessages.some(message =>
                message.includes('created in an older version of wynncraft'),
            ),
            beta_migration_prompted: confirmMessages.some(message =>
                message.includes("You're using an unofficial version of WynnBuilder"),
            ),
            confirm_messages: confirmMessages,
            navigation_error: navigationError,
            page_errors: pageErrors,
            console_errors: consoleErrors,
            error_text: state?.error_text || '',
            atree_warning: state?.atree_warning || '',
            ability_tree_valid: abilityTreeValid,
            ap_cost: state?.ap_cost ?? null,
            ap_cap: state?.ap_cap ?? null,
            loaded_weapon: state?.weapon || null,
            weapon_metadata_matches: Boolean(
                normalizedExpectedWeapon
                && normalizedActualWeapon
                && normalizedExpectedWeapon === normalizedActualWeapon
            ),
            has_effective_hp: state?.has_effective_hp || false,
            has_assigned_skillpoints: state?.has_assigned_skillpoints || false,
            elapsed_ms: Date.now() - started,
        });
    }

    return {
        schema_version: 1,
        validated_at: new Date().toISOString(),
        validator: 'Playwright against the live WynnBuilder UI, accepting its built-in old-version migration prompt',
        source_record_count: builds.length,
        results,
    };
}
