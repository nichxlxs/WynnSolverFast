"""Report WynnBuilder hrefs in the forum post that are not in the mapped corpus."""

from __future__ import annotations

import html
import json

import lxml.html

import extract_forum_builds as extractor


document = lxml.html.fromstring(extractor.fetch_html())
all_urls = {
    html.unescape(url)
    for url in document.xpath('//a[contains(@href, "wynnbuilder")]/@href')
    if "#" in url
}
payload = json.loads((extractor.DB_DIR / "functional-builds.json").read_text(encoding="utf-8"))
mapped_urls = {build["builder_url"] for build in payload["builds"]}
unmapped_urls = sorted(all_urls - mapped_urls)
contexts = []
for url in unmapped_urls:
    anchors = document.xpath('//a[@href=$target]', target=url)
    spoiler_text = None
    if anchors:
        spoilers = anchors[0].xpath(
            'ancestor::div[contains(concat(" ", normalize-space(@class), " "), " bbCodeSpoilerContainer ")][1]'
        )
        if spoilers:
            spoiler_text = extractor.clean_text(
                lxml.html.tostring(spoilers[0], encoding="unicode")
            )
    contexts.append(
        {
            "url": url,
            "anchor_text": extractor.clean_text(
                lxml.html.tostring(anchors[0], encoding="unicode")
            ) if anchors else None,
            "parent_text": extractor.clean_text(
                lxml.html.tostring(anchors[0].getparent(), encoding="unicode")
            ) if anchors else None,
            "spoiler_text": spoiler_text,
        }
    )
print(
    json.dumps(
        {
            "all_unique_wynnbuilder_urls": len(all_urls),
            "mapped_unique_wynnbuilder_urls": len(mapped_urls),
            "unmapped_unique_wynnbuilder_urls": len(unmapped_urls),
            "unmapped_urls": contexts,
        },
        indent=2,
    )
)
