## Reading the charts

Every chart reads the same way, and its legend names each mark.

| Mark | What it means |
|---|---|
| **Line** | The average over each point: a minute on the 1-hour range, a few minutes on longer ones (the legend says how many), a day on Trends (or the day's total, where the legend says "each day"). It ends at the latest reading. |
| **Band** | The busiest moment within each point, so a spike of a few seconds still shows. On Trends, each day's peak. |
| **Dashed line** | The **tidemark**, where a chart has one: the week's high (on Trends' disk space, the range's). Its label sits just above the line. |
| **Dot and label** | The range's peak. Click the dot to see that moment in the card on the right. |
| **Breathing dot** | **Now**: the latest reading, at the end of a line that runs up to the present. |
| **Thin vertical line** | The moment (or the day) the card on the right is about, on every chart of the tab. |
| **Soft band** | **Asleep**: nothing runs while the computer sleeps, so sleep is a gap, not a dip to zero. |
| **Hatched band** | **Not recording**: Tidemark was quit or not yet started. |

- Point at a chart to read any moment, or at a gap for whether the computer was asleep or not recording, and for how long. Click it to pick that moment.
- **Meters**: the lit segments on Now's cards and in the <!-- mac -->menu bar<!-- /mac --><!-- github --> (Mac) or <!-- /github --><!-- windows -->notification area<!-- /windows --><!-- github --> (Windows)<!-- /github --> window show how full each measure is now: CPU, GPU, memory and disk space against their total, rates against the hour's busiest moment, energy against the week's tidemark.
- **Colour** marks trouble. When the status is about a measure, its number, meter and line turn amber (a warning) or red (critical), with a word saying which. Several measures can be flagged at once; the headline is about the worst. Everything else stays in the tide colour.
