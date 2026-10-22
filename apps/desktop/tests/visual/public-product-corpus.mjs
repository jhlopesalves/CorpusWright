/**
 * Original prose for a fictional shared garden. These documents describe no real
 * people, sites or collected observations. Keep the authored passages intact:
 * recurrence should arise from the subject, not generated sentence loops.
 */
import { writeFileSync } from "node:fs";
import { join } from "node:path";

export const productPassages = [
  ["bed-plan.txt", `Planning the shared garden

The narrow bed beside the gate gets morning sun but falls into shade after lunch. Leave it for lettuce and parsley rather than tomatoes. The wider bed along the fence is warmer, although the soil there dries quickly. A strip of clover can remain between the beds until the planting dates are settled.

Before digging, check whether the ground crumbles or sticks to the spade. Walking on wet soil presses out the air and leaves hard lumps when it dries. Spread mature compost over the surface of the wider bed, then work it into the top layer with a fork. Keep the compost heap behind the shed accessible from both sides; a full wheelbarrow cannot turn in the present gap.

The proposed path is wide enough for one person carrying tools, but two people will struggle to pass. Move the corner post before putting down gravel. A short plank across the damp patch would help during spring work, provided it can be lifted for inspection.

There is room for a small seed tray on the shelf inside the shed. It should hold spare seedlings for replacing losses, rather than a second set of every crop. Label each row with the variety and sowing date. The labels already in the drawer have faded, so use pencil on fresh wooden markers.

Not everything needs to be decided now. Keep one bed unallocated until the early crops have been harvested. That gives the group somewhere to try a late sowing without disturbing plants that are still growing.`],
  ["sowing-guide.txt", `Sowing and transplanting

Read the packet before opening it. Some seeds need light to germinate and should stay near the surface; others need a shallow covering of soil. A seed tray with drainage holes is easier to manage than a deep pot, because excess water can escape before the roots become waterlogged.

Fill the seed tray loosely, tap it once on the bench and level the surface. Use fine seed compost for the smallest seeds. The compost heap supplies material for outdoor beds, but its coarse pieces are unsuitable for a tray of tiny seedlings. Do not press the mixture into a solid block.

Sow thinly enough to leave space between emerging plants. With larger seeds, a pencil makes a convenient hole at a consistent depth. Cover the tray and put the label where it can be read without lifting the lid. Check it each morning; condensation is useful, but pools of water are a reason to lift the cover briefly.

When seedlings have several leaves, loosen a small cluster with a spoon and hold each plant by a leaf. A damaged leaf can be replaced; a crushed stem usually cannot. Move each seedling into a pot with room for its roots, and water gently from below if the surface is liable to wash away.

Before planting outside, leave the pots in a sheltered position for gradually longer periods. Bring them back under cover if the night is cold. Transplant on a dull afternoon, and keep the empty seed tray for the next sowing rather than buying a new one.`],
  ["watering-plan.txt", `Watering during dry weather

The hose reaches the fence bed but stops short of the pots by the gate. Keep two watering cans there so nobody has to drag the hose across the planted rows. Put them upside down after use; a can left full is surprisingly heavy when the next person picks it up.

Water at the roots where possible. A light spray on the leaves may look helpful while leaving the soil dry underneath. Push a finger below the surface before deciding that a bed needs water. Pots require a separate check because wind and warm paving can dry them much faster than the ground.

New seedlings need a gentle flow. The rose on the smaller can gives a finer spray than the nozzle on the hose. A seed tray should never stand in a deep pool all afternoon. If the compost feels damp throughout, leave it until the following morning and check again.

Mulch can reduce evaporation after the soil has been watered. Use finished compost around established plants, keeping it away from their stems. Do not take unfinished material from the compost heap simply because the bucket nearby is empty.

The rota is a reminder to inspect the garden, not an instruction to water every bed every day. Anyone who finds a blocked drain or a leaking connector should leave a short note in the shed. State what was checked and what needs attention, so the next visit begins with a useful task rather than a guess.`],
  ["compost-guide.txt", `Looking after the compost heap

Collect vegetable peelings, spent plants and dry leaves in separate containers before carrying them outside. Separation makes it easier to mix wet and dry material at the compost heap. Remove plastic ties and labels from old stems; they will survive long after the plant has broken down.

A compost heap made entirely from grass cuttings can become a dense, wet mass. Add shredded cardboard or dry leaves as the cuttings arrive, and break up compacted layers with a fork. Thick woody branches belong in a separate pile until they can be chipped. Small twigs are useful at the base because they leave gaps for air.

The compost heap should feel damp rather than dripping. During a dry spell, add a little water while turning the outer layer inward. If liquid runs from the base, stop adding wet kitchen scraps for a while and mix in dry material. Keep the lid secure without sealing every opening.

Finished compost has a crumbly texture and an earthy smell. A few recognisable stems do not necessarily mean that the whole batch is unusable; sieve what is needed for smaller pots and return the coarse pieces to the compost heap. For outdoor beds, a less finely sifted mixture is usually easier to spread.

Reserve fine seed compost for the seed tray. Mature garden compost may be useful in a larger container, but seedlings are easier to inspect in a consistent, fine mixture. Label the bags clearly so the two materials are not confused.

Clean the fork and shovel before returning them to the shed. Put the empty bucket beside the collecting containers, with its handle upright. The next person should be able to continue the work without searching through a heap of tools.`],
  ["seed-swap.txt", `Preparing the seed swap table

Arrange unopened packets separately from seeds saved in the garden. For saved seed, write the crop, collection year and any uncertainty about the variety. An attractive envelope is less useful than a plain one with legible information. Leave a few empty envelopes and pencils at the end of the table.

A photograph of a plant helps people recognise it, but it does not guarantee that its seeds will produce the same appearance. Keep promises modest. Anyone who wants to try an unfamiliar crop can take a small portion before committing a whole bed to it.

The display seed tray should contain established seedlings rather than freshly sown examples. Visitors can then see the spacing between plants and compare the leaves without opening every packet. Bring a second seed tray for carrying the spare pots, and put a cloth underneath it to protect the table.

Explain where different growing materials are stored. Fine seed compost is in the labelled bag; the compost heap is outside, behind the shed. People collecting compost for a garden bed should bring a bucket and use the finished batch, leaving the newer material alone.

Keep the path through the room clear. The table near the door is convenient for conversation but awkward for anyone arriving with a pushchair. Move the empty boxes under the bench and leave chairs at both ends. At closing time, count the remaining packets for the next event; the number is an inventory, not a measure of how successfully the plants will grow.`],
  ["path-repairs.txt", `Repairing the path and shed

The loose board at the shed entrance catches the wheelbarrow. Remove it before unloading anything heavy, and check the support beneath it rather than replacing the visible timber alone. The door also needs a small adjustment: its lower corner drags when the frame is damp.

There is a shallow hollow in the gravel path beside the compost heap. Rain collects there and splashes against the bottom panel. Lift the displaced stones, fill the hollow and make a slight slope away from the timber. Do not raise the whole path above the edge of the bed, where it would send water onto the plants.

Keep a clear working area around the compost heap while repairs are under way. The fork can rest against the fence temporarily, but the saw belongs on the bench with its teeth covered. Loose screws should go into a jar, not into the bucket used for kitchen scraps.

The shelf for the seed tray needs a second bracket. Its existing screws hold in thin boarding rather than the frame, so a tray full of damp compost could pull it down. Fix the bracket to solid timber and test the shelf with an empty tray before loading it.

Finish by sweeping the entrance and checking that the door closes without force. Leave a note identifying any task that remains unfinished. A clear description of a loose hinge is more useful than a general warning that the shed needs work.`],
  ["season-diary.txt", `Notes from the changing season

In early spring the beds looked empty from the gate, although shoots were already appearing close to the ground. The parsley survived beside the sheltered wall. Several labels had disappeared, and the broad leaves near the fence were easy to recognise only after looking beneath the taller weeds.

We moved the seed tray to the brighter shelf and turned it every few days. The seedlings leaned towards the window when it stayed in one position. A second tray remained outside during mild afternoons; carrying it back into the shed became part of the evening routine.

By late spring the clover along the path had spread into the first bed. Cutting it back exposed an uneven edge that had been hidden by foliage. The compost heap was easier to reach after the corner post was moved, and there was finally room to put down a full bucket without blocking the path.

During the hot spell, the pots needed attention before the outdoor beds. We used mature compost as a surface mulch around the beans and checked underneath it before watering. The compost heap itself dried at the edges, so the next turn included a little water and the leaves collected earlier.

Towards autumn, fallen fruit attracted wasps near the gate. We cleared the path and left sound fruit on the picking table. Some crops continued producing long after their neighbours had been removed. A plan drawn in winter cannot show every change; the dated notes help explain why a bed was left occupied or cleared earlier than expected.`],
  ["harvest-kitchen.txt", `Sorting the harvest for the kitchen

Put damaged vegetables on one side before weighing what will be stored. A split carrot may still be useful for soup, while a soft patch needs closer inspection. Brush loose soil from roots outdoors rather than washing everything at once. Wet vegetables are harder to store safely in the small cupboard.

The beans are easiest to sort on a clean cloth. Keep a bowl for sound pods and a separate container for trimmings. Thin the largest bunches before carrying them into the kitchen; a tightly packed bag hides bruising and holds moisture against the leaves.

Take peelings and discarded leaves to the compost heap after food preparation. Remove stickers from fruit skins and keep cooked scraps out of the collection bucket. A quick rinse leaves the bucket ready for the next person without filling it with unnecessary water.

Fresh herbs need little preparation. Shake out insects, remove damaged stems and chop only what is needed for the meal. Parsley from the shaded bed has longer stalks than the small plants by the gate, but both can be used in the same dish.

Leave a short list of stored crops beside the cupboard. Record what is there and which items should be used first. The list can guide the next meal without pretending to forecast how long every vegetable will keep.`],
];

export function generateProductCorpus(directory) {
  for (const [name, text] of productPassages) {
    writeFileSync(join(directory, name), `${text}\n`);
  }
}
