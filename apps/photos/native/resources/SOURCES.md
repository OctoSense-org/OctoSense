# Sample photo provenance

The six individual portraits are exact copies of the user-provided/generated files in `scripts/individuals/`. `family.png` is copied from `scripts/generated.png`. The originals are preserved.

The landscape JPEGs were downloaded from Unsplash on September 16, 2026. Download parameters: `auto=format&fit=crop&w=1200&q=82&fm=jpg`. See the [Unsplash license](https://unsplash.com/license). Photos are bundled for offline sample browsing.

| File | Source image |
| --- | --- |
| `mountain.jpg` | [Unsplash image](https://images.unsplash.com/photo-1454496522488-7a8e488e8606) |
| `alpine.jpg` | [Unsplash image](https://images.unsplash.com/photo-1506905925346-21bda4d32df4) |
| `night-sky.jpg` | [Unsplash image](https://images.unsplash.com/photo-1519681393784-d120267933ba) |
| `ocean.jpg` | [Unsplash image](https://images.unsplash.com/photo-1518837695005-2083093ee35b) |
| `beach.jpg` | [Unsplash image](https://images.unsplash.com/photo-1507525428034-b723cf961d3e) |
| `coast.jpg` | [Unsplash image](https://images.unsplash.com/photo-1475924156734-496f6cac6ec1) |
| `forest.jpg` | [Unsplash image](https://images.unsplash.com/photo-1441974231531-c6227db76b6e) |
| `woodland.jpg` | [Unsplash image](https://images.unsplash.com/photo-1472396961693-142e6e269027) |
| `meadow.jpg` | [Unsplash image](https://images.unsplash.com/photo-1500534623283-312aade485b7) |
| `lake.jpg` | [Unsplash image](https://images.unsplash.com/photo-1470770841072-f978cf4d019e) |
| `sunlit-woods.jpg` | [Unsplash image](https://images.unsplash.com/photo-1447752875215-b2761acb3c5d) |
| `desert.jpg` | [Unsplash image](https://images.unsplash.com/photo-1509316785289-025f5b846b35) |

Names, dates, places, titles, and groupings in `catalog.json` are demonstration metadata, not inferred identities or original capture metadata. People collections use explicit catalog tags; the app does not perform face recognition.

## Everyday scenes generated September 18, 2026

The 24 PNGs below were generated with the built-in `image_gen` tool using the
six original individual portraits as identity references. Each image was generated
separately, with all people in group images explicitly referenced. The original
portraits are unchanged. These are synthetic sample photographs; their names,
dates, locations and events are demonstration metadata.

The exact prompt, ordered reference files and catalog metadata for every image
are recorded in [generated-scenes.json](generated-scenes.json), together with
final file hashes and the three framing-refinement prompts. Files are copied
without modification from the generated originals into `photos/`.

| File | People | Setting |
| --- | --- | --- |
| `alex-office.png` | Alex | The office |
| `alex-forest.png` | Alex | Woodland trail |
| `alex-stadium.png` | Alex | Local stadium |
| `sofia-office.png` | Sofia | The office |
| `sofia-beach.png` | Sofia | By the coast |
| `sofia-market.png` | Sofia | Town market |
| `james-garden.png` | James | Botanical garden |
| `james-harbor.png` | James | By the coast |
| `james-library.png` | James | Neighborhood library |
| `rose-garden.png` | Rose | Botanical garden |
| `rose-pier.png` | Rose | By the coast |
| `rose-market.png` | Rose | Town market |
| `noah-school.png` | Noah | School courtyard |
| `noah-football.png` | Noah | Local stadium |
| `noah-trail.png` | Noah | Woodland trail |
| `lily-school.png` | Lily | School courtyard |
| `lily-beach.png` | Lily | By the coast |
| `lily-meadow.png` | Lily | Woodland trail |
| `alex-sofia-cafe.png` | Alex, Sofia | Neighborhood cafe |
| `james-rose-garden.png` | James, Rose | Botanical garden |
| `alex-noah-lily-stadium-wide.png` | Alex, Noah, Lily | Local stadium |
| `sofia-rose-lily-market-wide.png` | Sofia, Rose, Lily | Town market |
| `family-four-beach-wide.png` | Alex, Sofia, Noah, Lily | By the coast |
| `grandparents-children-trail.png` | James, Rose, Noah, Lily | Woodland trail |

## Royalty-free stock photos added September 18, 2026

These 32 photographs were downloaded from Pexels under the
[Pexels License](https://www.pexels.com/license/), which permits free use in apps
and does not require attribution. Photographer credits are retained below.
The photos are offline demonstration content for the Photos app.

Images retain their original aspect ratio and are downloaded as JPEGs with a
maximum dimension of 1200 pixels. [stock-photos.json](stock-photos.json) records
every source page, photographer, download URL, license URL, size and SHA-256.
The catalog assigns eight sample dates to each of June, July, August and
September 2026; these are demonstration dates, not original capture dates.
Strangers in stock photos are not assigned the six sample family identities.

| File | Photographer | Source | Sample date |
| --- | --- | --- | --- |
| `stock-pets-at-play.jpg` | Foden Nguyen | [Pexels 9952105](https://www.pexels.com/photo/photo-of-cat-and-dog-9952105/) | 2026-06-03 |
| `stock-street-companions.jpg` | Kenneth Surillo | [Pexels 20849770](https://www.pexels.com/photo/dog-and-cat-on-a-street-20849770/) | 2026-07-02 |
| `stock-elephant-waterhole.jpg` | Timon Cornelissen | [Pexels 31481939](https://www.pexels.com/photo/african-elephant-near-waterhole-in-natural-habitat-31481939/) | 2026-08-03 |
| `stock-elephant-egret.jpg` | ἐμμανυελ  | [Pexels 12118214](https://www.pexels.com/photo/bird-sitting-on-top-of-an-elephant-12118214/) | 2026-09-01 |
| `stock-glass-skybridge.jpg` | Maks057Kh | [Pexels 29079181](https://www.pexels.com/photo/modern-architectural-bridge-between-skyscrapers-29079181/) | 2026-06-07 |
| `stock-bridge-arches.jpg` | Chris | [Pexels 17305502](https://www.pexels.com/photo/bridge-with-arch-17305502/) | 2026-07-06 |
| `stock-butterfly-wildflowers.jpg` | Matteo Badini | [Pexels 9365620](https://www.pexels.com/photo/butterfly-on-flowers-9365620/) | 2026-08-07 |
| `stock-sunset-sailboat.jpg` | Zbigniew Bielecki | [Pexels 1879545](https://www.pexels.com/photo/sailboat-sailing-on-sea-1879545/) | 2026-09-03 |
| `stock-downtown-avenue.jpg` | Wolf Art | [Pexels 26612797](https://www.pexels.com/photo/photo-of-a-city-street-26612797/) | 2026-06-10 |
| `stock-neighborhood-street.jpg` | Ilya Batorshin | [Pexels 14359709](https://www.pexels.com/photo/photo-of-a-city-street-14359709/) | 2026-07-10 |
| `stock-railway-yard.jpg` | Nicolò Pais | [Pexels 13088097](https://www.pexels.com/photo/train-on-the-railway-13088097/) | 2026-08-11 |
| `stock-train-journey.jpg` | Rahul | [Pexels 1023029](https://www.pexels.com/photo/photo-of-train-on-railway-1023029/) | 2026-09-05 |
| `stock-fruit-market.jpg` | Bruno Pedro | [Pexels 13644025](https://www.pexels.com/photo/fresh-fruits-in-the-market-13644025/) | 2026-06-14 |
| `stock-summer-fruit.jpg` | Rachel Claire | [Pexels 4819321](https://www.pexels.com/photo/fresh-fruits-in-close-up-shot-4819321/) | 2026-07-14 |
| `stock-coffee-camera.jpg` | Engin Akyurt | [Pexels 2347304](https://www.pexels.com/photo/close-up-photo-of-coffee-cup-2347304/) | 2026-08-15 |
| `stock-morning-coffee.jpg` | Samer Daboul | [Pexels 1252194](https://www.pexels.com/photo/close-up-photography-of-coffee-cup-1252194/) | 2026-09-07 |
| `stock-mossy-waterfall.jpg` | Ray Bilcliff | [Pexels 2904636](https://www.pexels.com/photo/waterfalls-2904636/) | 2026-06-18 |
| `stock-forest-cascade.jpg` | Surdu Horia | [Pexels 27927855](https://www.pexels.com/photo/waterfall-27927855/) | 2026-07-18 |
| `stock-mountain-walk.jpg` | Vadir Camargo | [Pexels 15336495](https://www.pexels.com/photo/people-hiking-in-mountains-15336495/) | 2026-08-19 |
| `stock-hiking-together.jpg` | Ali Alcántara | [Pexels 35746721](https://www.pexels.com/photo/hiking-adventure-in-mountain-landscape-35746721/) | 2026-09-09 |
| `stock-court-lines.jpg` | Ala J Graczyk | [Pexels 7864342](https://www.pexels.com/photo/photo-of-a-basketball-court-ground-7864342/) | 2026-06-21 |
| `stock-city-basketball.jpg` | PNW Production | [Pexels 8980698](https://www.pexels.com/photo/basketball-court-during-daytime-8980698/) | 2026-07-22 |
| `stock-bicycle-benches.jpg` | 27 1 | [Pexels 15540091](https://www.pexels.com/photo/bicycle-parked-by-the-benches-15540091/) | 2026-08-23 |
| `stock-city-bicycles.jpg` | Mariya Muschard | [Pexels 12905556](https://www.pexels.com/photo/photo-of-bicycle-parked-near-bench-12905556/) | 2026-09-11 |
| `stock-library-shelves.jpg` | Polina Zimmerman | [Pexels 3747514](https://www.pexels.com/photo/books-in-library-3747514/) | 2026-06-25 |
| `stock-breakfast-table.jpg` | Arina Krasnikova | [Pexels 7005212](https://www.pexels.com/photo/photo-of-fruits-near-a-loaf-of-bread-7005212/) | 2026-07-26 |
| `stock-garden-zinnias.jpg` | Andrew Patrick Photo | [Pexels 9321208](https://www.pexels.com/photo/a-photo-of-colorful-flowers-9321208/) | 2026-08-27 |
| `stock-garden-petals.jpg` | Matheus Bertelli | [Pexels 20233806](https://www.pexels.com/photo/colorful-flowers-in-a-garden-20233806/) | 2026-09-14 |
| `stock-home-guitar.jpg` | Andrea Piacquadio | [Pexels 3931073](https://www.pexels.com/photo/man-playing-the-guitar-3931073/) | 2026-06-28 |
| `stock-music-practice.jpg` | Elina Sazonova | [Pexels 3971985](https://www.pexels.com/photo/person-playing-guitar-with-musical-notes-3971985/) | 2026-07-30 |
| `stock-koi-pond.jpg` | Mo Eid | [Pexels 18916437](https://www.pexels.com/photo/colorful-fish-in-water-18916437/) | 2026-08-30 |
| `stock-aquarium-fish.jpg` | Mehmet Turgut Kirkgoz | [Pexels 18394485](https://www.pexels.com/photo/colorful-fish-swimming-18394485/) | 2026-09-17 |
