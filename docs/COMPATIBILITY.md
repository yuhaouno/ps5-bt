# Compatibility evidence

No PS5, firmware, headset, codec or controller family is Verified by this project.
Marvell/NXP and MediaTek are Experimental research targets only. All console
models and versions remain Unknown. There are no measured latency numbers.

| Label | Required meaning |
| --- | --- |
| Verified | reproducible project run, exact build and environment, attached sanitized evidence |
| Reported | identifiable third-party observation with provenance and limitations |
| Experimental | implementation/experiment exists or a research target is being investigated; not supported |
| Unsupported | explicitly unavailable in the current implementation or explicitly rejected |
| Unknown | no sufficient evidence; never equivalent to incompatible |

Upstream observations remain Reported about upstream only. Controller identity,
console model, firmware and headset model are separate dimensions. Do not fill
a product support table from marketing specifications or a VID alone.

Hardware reports must include the fields in [HARDWARE-TEST.md](HARDWARE-TEST.md),
plus headset/firmware, pairing history, negotiated codec parameters, disconnect
cause, reconnect attempt counts, playback duration and workload for audio tests.
Mark missing fields Unknown. A successful tone does not verify game capture,
headset power-cycle reconnect, Wi-Fi coexistence, pad reconnect, or an 8 h soak.
