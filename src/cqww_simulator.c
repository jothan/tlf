/*
 * Tlf - contest logging program for amateur radio operators
 * Copyright (C) 2001-2002-2003 Rein Couperus <pa0rct@amsat.org>
 *               2011, 2014     Thomas Beierlein <tb@forth-ev.de>
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
 * Foundation, Inc., 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA
 */

#include <string.h>

#include "cqww_simulator.h"
#include "get_time.h"
#include "getctydata.h"
#include "globalvars.h"
#include "searchlog.h"		// Includes glib.h
#include "sendbuf.h"
#include "set_tone.h"
#include "tlf.h"
#include "rust.h"

/* CW Simulator
 * works only in RUN mode for CQWW contest
 */

bool simulator = false;

static simstate_t simulator_state = IDLE;
static pthread_mutex_t simulator_state_mutex = PTHREAD_MUTEX_INITIALIZER;

simstate_t get_simulator_state() {
    if (!simulator || trxmode != CWMODE) {
	return IDLE;
    }

    pthread_mutex_lock(&simulator_state_mutex);
    simstate_t s = simulator_state;
    pthread_mutex_unlock(&simulator_state_mutex);

    return s;
}

void set_simulator_state(simstate_t s) {
    if (!simulator || trxmode != CWMODE) {
	return;
    }

    pthread_mutex_lock(&simulator_state_mutex);
    simulator_state = s;
    pthread_mutex_unlock(&simulator_state_mutex);
}

const int cw_tones[] = {
    625, 800, 650, 750, 700,
    725, 675, 775, 600, 640
};
#define NUM_TONES (sizeof(cw_tones) / sizeof(int))


static int simulator_tone;
static int tonecpy;

static void set_simulator_tone(void) {
    tonecpy = write_tone(simulator_tone);
    sendmessage("  ");
}

static void restore_tone(void) {
    write_tone(tonecpy);
}

void cqww_simulator(void) {

    if (!simulator) {
	return;
    }

    static int repeat_count = 0;

    char callcpy[80];

    simstate_t state = get_simulator_state();

    if (state == CALL) {

	int this_second = get_time() % 60;

       simulator_tone = cw_tones[this_second % NUM_TONES];

	set_simulator_tone();

        callmaster_pick_random();

	sendmessage(callmaster_random_call());

	repeat_count = 0;
	restore_tone();

    } else if (state == FINAL) {

	set_simulator_tone();

	strcpy(callcpy, callmaster_random_call());
	getctydata(callcpy);

	char save = cqzone[0];
	if (get_time() % 2 == 0) {  // use short numbers randomly
	    cqzone[0] = short_number(cqzone[0]);
	}

	char *str = g_strdup_printf("TU 5NN %s", cqzone);
	sendmessage(str);
	g_free(str);
	cqzone[0] = save;

	repeat_count = 0;
	restore_tone();

    } else if (state == REPEAT) {

	set_simulator_tone();

	++repeat_count;
	int slow = repeat_count / 2;
	if (slow > 3) {
	    slow = 3;
	}

	strcpy(callcpy, callmaster_random_call());
	getctydata(callcpy);

	char *str = g_strdup_printf("%s%s%s",
				    &"---"[3 - slow],
				    callmaster_random_call(),
				    &"+++"[3 - slow]);
	sendmessage(str);
	g_free(str);

	restore_tone();
    }

    set_simulator_state(IDLE);

}
