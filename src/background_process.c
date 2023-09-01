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
 * Foundation, Inc., 51 Franklin Street, Fifth Floor, Boston, MA  02110-1301 USA
 */


#include <assert.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include "background_process.h"
#include "err_utils.h"
#include "fldigixmlrpc.h"
#include "get_time.h"
#include "gettxinfo.h"
#include "lancode.h"
#include "log_to_disk.h"
#include "qsonr_to_str.h"
#include "qtc_log.h"
#include "qtcutil.h"
#include "qtcvars.h"
#include "rtty.h"
#include "splitscreen.h"
#include "tlf.h"
#include "rust.h"


void handle_lan_recv(int *lantimesync) {
    char *prmessage;
    char lan_message[256];

    if(!lan_active || lan_recv(lan_message) == -1)
        return;

    if (landebug && strlen(lan_message) > 2) {
        FILE *fp;
        char debugbuffer[160];

        if ((fp = fopen("debuglog", "a")) == NULL) {
            printf("store_qso.c: Error opening debug file.\n");
        } else {
            format_time(debugbuffer, sizeof(debugbuffer), "%H:%M:%S-");
            fputs(debugbuffer, fp);
            fputs(lan_message, fp);
            fputs("\n", fp);
            fclose(fp);
        }
    }
    if(strlen(lan_message) == 0) {
        return;
    }

    if (lan_message[0] == thisnode) {
        TLF_LOG_WARN("%s", "Warning: NODE ID CONFLICT ?! You should use another ID! ");
    }

    if (lan_message[0] != thisnode && !is_background_process_stopped()) {
        switch (lan_message[1]) {

            case LOGENTRY:

                log_to_disk(lan_message);
                break;

            case QTCRENTRY:

                store_qtc(lan_message + 2, RECV, QTC_RECV_LOG);
                break;

            case QTCSENTRY:

                store_qtc(lan_message + 2, SEND, QTC_SENT_LOG);
                break;

            case QTCFLAG:

                parse_qtc_flagline(lan_message + 2);
                break;

            case CLUSTERMSG:
                prmessage = g_strndup(lan_message + 2, 80);
                if (strstr(prmessage, my.call) != NULL) {	// alert for cluster messages
                    TLF_LOG_INFO(prmessage);
                }

                addtext(prmessage);
                g_free(prmessage);
                break;
            case TLFSPOT:
                prmessage = g_strndup(lan_message + 2, 80);
                lanspotflg = true;
                addtext(prmessage);
                lanspotflg = false;
                g_free(prmessage);
                break;
            case TLFMSG:
                for (int t = 0; t < 4; t++)
                    strcpy(talkarray[t], talkarray[t + 1]);

                talkarray[4][0] = lan_message[0];
                talkarray[4][1] = ':';
                talkarray[4][2] = '\0';
                g_strlcat(talkarray[4], lan_message + 2,
                            sizeof(talkarray[4]));
                TLF_LOG_INFO(" MSG from %s", talkarray[4]);
                break;
            case FREQMSG:
                if ((lan_message[0] >= 'A')
                        && (lan_message[0] <= 'A' + MAXNODES)) {
                    node_frequencies[lan_message[0] - 'A'] =
                        atof(lan_message + 2) * 1000.0;
                    break;
                }
            case INCQSONUM:
                int n = atoi(lan_message + 2);

                if (highqsonr < n)
                    highqsonr = n;

                if ((qsonum <= n) && (n > 0)) {
                    qsonum = highqsonr + 1;
                    qsonr_to_str(qsonrstr, qsonum);
                }
                lan_message[0] = '\0';

            case TIMESYNC:
                if ((lan_message[0] >= 'A')
                        && (lan_message[0] <= 'A' + MAXNODES)) {
                    time_t lantime = atoi(lan_message + 2);

                    time_t delta = lantime - (get_time() - timecorr);

                    if (*lantimesync == 1) {
                        timecorr = (4 * timecorr + delta) / 5;
                    } else {
                        timecorr = delta;
                        *lantimesync = 1;
                    }

                    break;
                }
        }
   }
}