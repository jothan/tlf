/*
 * Tlf - contest logging program for amateur radio operators
 * Copyright (C) 2001-2002-2003 Rein Couperus <pa0rct@amsat.org>
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


#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <assert.h>

#include "bands.h"
#include "err_utils.h"
#include "sendqrg.h"
#include "startmsg.h"
#include "gettxinfo.h"
#include "bands.h"
#include "globalvars.h"
#include "rust.h"

void send_bandswitch(freq_t trxqrg);

/* check if call input field contains a frequency value and switch to it.
 *
 */
int sendqrg(void) {

    if (!trx_control) {
	return 0;               /* nothing to do here */
    }

    const freq_t trxqrg = atof(current_qso.call) * 1000.0;

    int bandinx = freq2bandindex(trxqrg);

    if (bandinx == BANDINDEX_OOB) {
	return 0;   // not a frequency or out of band
    }

    set_outfreq(trxqrg);
    send_bandswitch(trxqrg);

    return trxqrg;
}


/*static void debug_tlf_rig() {
    freq_t rigfreq;
    int retcode;

    sleep(10);

    pthread_mutex_lock(&rig_lock);
    retcode = rig_get_freq(my_rig, RIG_VFO_CURR, &rigfreq);
    pthread_mutex_unlock(&rig_lock);

    if (retcode != RIG_OK) {
	TLF_LOG_WARN("Problem with rig get freq: %s", rigerror(retcode));
    } else {
	shownr("freq =", (int) rigfreq);
    }
    sleep(10);

    const freq_t testfreq = 14000000;	// test set frequency

    pthread_mutex_lock(&rig_lock);
    retcode = rig_set_freq(my_rig, RIG_VFO_CURR, testfreq);
    pthread_mutex_unlock(&rig_lock);

    if (retcode != RIG_OK) {
	TLF_LOG_WARN("Problem with rig set freq: %s", rigerror(retcode));
    } else {
	showmsg("Rig set freq ok!");
    }

    pthread_mutex_lock(&rig_lock);
    retcode = rig_get_freq(my_rig, RIG_VFO_CURR, &rigfreq);	// read qrg
    pthread_mutex_unlock(&rig_lock);

    if (retcode != RIG_OK) {
	TLF_LOG_WARN("Problem with rig get freq: %s", rigerror(retcode));
    } else {
	shownr("freq =", (int) rigfreq);
	if (rigfreq != testfreq) {
	    showmsg("Failed to set rig freq!");
	}
    }
    sleep(10);

}
*/