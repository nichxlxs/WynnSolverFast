async page => {
    const seeds = [
        { family: 'cancelstack', weapon: 'Trance', url: 'https://wynnbuilder.github.io/builder/#CW0rkkvWHycp3ZRgpUnRgZfHCDIvHamn8lRgpmWW1W+tjqzDJMm2' },
        { family: 'heavy_melee', weapon: 'Vengeance', url: 'https://wynnbuilder.github.io/builder/#CW04KQuCA3c1HXX6EE0n1827XD8xDRmna0m-ybq-j82BB' },
        { family: 'tierstack', weapon: 'Fate', url: 'https://wynnbuilder-beta.github.io/builder/#CT0i21s1Cn3OVWw4CDI15amn8q6fpb0WsxDL3NNv4a2' },
        { family: 'spellsteal', weapon: 'Oblivion', url: 'https://wynnbuilder.github.io/builder/#CW0bum9DaE1yQGoDqm1KEml1GkCuK98O0YN-H-XrV-E4' },
        { family: 'spell_sustained', weapon: 'Divzer', url: 'https://wynnbuilder.github.io/builder/#CW0o2tm9im6EfJsm1jv6E2QGG3OOG09TQsmfRGB1m-exVq7SEa30' },
        { family: 'hybrid', weapon: 'Divzer', url: 'https://wynnbuilder.github.io/builder/#CW0cVAwCufe3dWYwZlcYwZfHCD266M1GdcDSM7qI830qFwzFuXx9S8' },
    ];
    await page.context().addInitScript(() => {
        window.__wynn_validation_confirm_messages = [];
        window.confirm = message => {
            window.__wynn_validation_confirm_messages.push(String(message));
            return String(message).includes('created in an older version of wynncraft')
                || String(message).includes("You're using an unofficial version of WynnBuilder");
        };
    });
    const results = [];
    for (const seed of seeds) {
        let navigationError = null;
        const pageErrors = [];
        const onPageError = error => pageErrors.push(error.message);
        page.on('pageerror', onPageError);
        try {
            await page.goto('about:blank');
            await page.goto(seed.url, { waitUntil: 'domcontentloaded', timeout: 20000 });
            await page.waitForFunction(() => {
                const weapon = [...document.querySelectorAll('input')]
                    .find(input => input.placeholder === 'No Weapon');
                return Boolean(weapon?.value || document.querySelector('#err-box')?.textContent?.trim());
            }, null, { timeout: 12000 });
            await page.waitForTimeout(350);
        } catch (error) {
            navigationError = error.message;
        }
        const state = await page.evaluate(() => {
            const inputs = [...document.querySelectorAll('input')];
            const weapon = inputs.find(input => input.placeholder === 'No Weapon')?.value || null;
            const apCostText = document.querySelector('#active_AP_cost')?.textContent?.trim() || '';
            const apCapText = document.querySelector('#active_AP_cap')?.textContent?.trim() || '';
            const levelText = inputs.find(input => input.placeholder === 'Build level')?.value || '';
            return {
                current_game_version: wynn_version_names[WYNN_VERSION_LATEST],
                weapon,
                level: /^\d+$/.test(levelText) ? Number(levelText) : null,
                error_text: document.querySelector('#err-box')?.textContent?.trim() || '',
                atree_warning: document.querySelector('#atree-warning')?.textContent?.trim() || '',
                ap_cost: /^\d+$/.test(apCostText) ? Number(apCostText) : null,
                ap_cap: /^\d+$/.test(apCapText) ? Number(apCapText) : null,
                confirm_messages: window.__wynn_validation_confirm_messages || [],
                has_effective_hp: document.body.innerText.includes('Effective HP:'),
            };
        });
        page.off('pageerror', onPageError);
        const apUtilization = state.ap_cap > 0 ? state.ap_cost / state.ap_cap : 0;
        const valid = Boolean(
            !navigationError
            && !state.error_text
            && !state.atree_warning
            && pageErrors.length === 0
            && state.weapon
            && state.has_effective_hp
            && apUtilization >= 0.8
        );
        results.push({
            ...seed,
            source_url: seed.url,
            final_url: page.url(),
            status: valid ? 'current_functional' : 'current_broken_or_incomplete',
            valid_current_builder: valid,
            ability_tree_valid: !state.atree_warning && state.ap_cost <= state.ap_cap,
            ability_tree_completeness_proxy: apUtilization,
            navigation_error: navigationError,
            page_errors: pageErrors,
            ...state,
        });
    }
    return {
        schema_version: 1,
        validated_at: new Date().toISOString(),
        validator: 'Playwright against the live official WynnBuilder after automatic migration to its current data',
        source_record_count: seeds.length,
        results,
    };
}
