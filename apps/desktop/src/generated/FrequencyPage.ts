import type { FrequencyTermItem } from "./FrequencyTermItem.js";
import type { FrequencyNotice } from "./FrequencyNotice.js";

export type FrequencyPage = { totalTerms: number, scopedTotalTypes: number, offset: number, limit: number, items: Array<FrequencyTermItem>, selectedDocumentCount: number, totalDocumentCount: number, scopedTotalTokens: number, queryError: string | null, notices: Array<FrequencyNotice>, };
