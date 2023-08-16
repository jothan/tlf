/*
 * Tlf - contest logging program for amateur radio operators
 * Copyright (C) 2001-2002-2003-2004 Rein Couperus <pa0r@amsat.org>
 *
 * This program is free software; you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation; either version 2 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program; if not, write to the Free Software
 * Foundation, Inc., 51 Franklin Street, Fifth Floor, Boston, MA  02110-1301 USA
 */

/* ------------------------------------------------------------
 *              Parse various  call  formats
 *              Convert country data
 *--------------------------------------------------------------*/


#include <ctype.h>
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <stdbool.h>
#include "getpx.h"
#include "globalvars.h"		// Includes glib.h and tlf.h
#include "setcontest.h"
#include "rust.h"

/* lookup dxcc country and prefix information from callsign */
const prefix_data *getctyinfo(char *call) {
    int w = getpfxindex(call, NULL);
    return prefix_by_index(w);
}

/* lookup various dxcc cty data from callsign
 *
 * side effect: set up various global variables
 */
static int getctydata_internal(const char *call, bool get_country) {
    char *normalized_call = NULL;

    int w = getpfxindex(call, &normalized_call);

    if (CONTEST_IS(WPX) || pfxmult)
	/* needed for wpx and other pfx contests */
	getpx(normalized_call);

    free(normalized_call);

    // fill global variables
    const prefix_data *pfx = prefix_by_index(w);
    countrynr = pfx->dxcc_ctynr;
    sprintf(cqzone, "%02d", pfx->cq);
    sprintf(ituzone, "%02d", pfx->itu);
    DEST_Lat = pfx->lat;
    DEST_Long = pfx->lon;

    g_strlcpy(continent, pfx->continent, 3);

    return get_country ? countrynr : w;
}

int getctydata(const char *call) {
    return getctydata_internal(call, true);
}

int getctydata_pfx(const char *call) {
    return getctydata_internal(call, false);
}
