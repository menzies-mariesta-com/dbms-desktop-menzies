/**
 * Lucide icon nodes for Svelte, matching Wash UI's lucide-react 1.28.0 pin.
 * Sync `__iconNode` imports only (no top-level await on boot path).
 */
import { __iconNode as bookmark } from 'lucide-react/dist/esm/icons/bookmark.mjs';
import { __iconNode as check } from 'lucide-react/dist/esm/icons/check.mjs';
import { __iconNode as chevronDown } from 'lucide-react/dist/esm/icons/chevron-down.mjs';
import { __iconNode as chevronRight } from 'lucide-react/dist/esm/icons/chevron-right.mjs';
import { __iconNode as circlePlay } from 'lucide-react/dist/esm/icons/circle-play.mjs';
import { __iconNode as columns3 } from 'lucide-react/dist/esm/icons/columns-3.mjs';
import { __iconNode as database } from 'lucide-react/dist/esm/icons/database.mjs';
import { __iconNode as databaseZap } from 'lucide-react/dist/esm/icons/database-zap.mjs';
import { __iconNode as download } from 'lucide-react/dist/esm/icons/download.mjs';
import { __iconNode as fileCode } from 'lucide-react/dist/esm/icons/file-code.mjs';
import { __iconNode as folder } from 'lucide-react/dist/esm/icons/folder.mjs';
import { __iconNode as gitCompare } from 'lucide-react/dist/esm/icons/git-compare.mjs';
import { __iconNode as history } from 'lucide-react/dist/esm/icons/rotate-ccw-clock.mjs';
import { __iconNode as network } from 'lucide-react/dist/esm/icons/network.mjs';
import { __iconNode as plus } from 'lucide-react/dist/esm/icons/plus.mjs';
import { __iconNode as refreshCw } from 'lucide-react/dist/esm/icons/refresh-cw.mjs';
import { __iconNode as settings } from 'lucide-react/dist/esm/icons/settings.mjs';
import { __iconNode as square } from 'lucide-react/dist/esm/icons/square.mjs';
import { __iconNode as sun } from 'lucide-react/dist/esm/icons/sun.mjs';
import { __iconNode as table } from 'lucide-react/dist/esm/icons/table.mjs';
import { __iconNode as trash2 } from 'lucide-react/dist/esm/icons/trash-2.mjs';
import { __iconNode as minus } from 'lucide-react/dist/esm/icons/minus.mjs';
import { __iconNode as x } from 'lucide-react/dist/esm/icons/x.mjs';
import { __iconNode as pencil } from 'lucide-react/dist/esm/icons/pencil.mjs';
import { __iconNode as plug } from 'lucide-react/dist/esm/icons/plug.mjs';
import { __iconNode as search } from 'lucide-react/dist/esm/icons/search.mjs';
import type { WashIconNode } from '$lib/tool/wash-icon-node';

export type { WashIconNode };

export const washIcons = {
	sun,
	download,
	'refresh-cw': refreshCw,
	minus,
	square,
	x,
	check,
	'chevron-down': chevronDown,
	'chevron-right': chevronRight,
	database,
	'database-zap': databaseZap,
	plus,
	table,
	folder,
	'file-code': fileCode,
	'circle-play': circlePlay,
	history,
	settings,
	network,
	'git-compare': gitCompare,
	bookmark,
	'trash-2': trash2,
	pencil,
	plug,
	search,
	'columns-3': columns3
} as const satisfies Record<string, WashIconNode>;
