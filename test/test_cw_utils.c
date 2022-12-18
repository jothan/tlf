#include "test.h"
#include "../src/tlf.h"

#include "../rustlf/rust.h"

// OBJECT ../src/dummy.o

const unsigned int bandcorner[NBANDS][2] = {
    { 1800000, 2000000 },	// band bottom, band top
    { 3500000, 4000000 },
    { 5250000, 5450000 },       // 5351500-5356500 worldwide
    { 7000000, 7300000 },
    { 10100000, 10150000 },
    { 14000000, 14350000 },
    { 18068000, 18168000 },
    { 21000000, 21450000 },
    { 24890000, 24990000 },
    { 28000000, 29700000 },
    {        0,        0 }
};


void test_SetSpeed_success(void **state) {
    for (int i = 4; i <= 66; ++i) {

	SetCWSpeed(i);

	int expected = (i - 9) / 2;     // for 11..50

	// special cases:
	//  - low speeds
	if (i <= 6) {
	    expected =  0;
	} else if (i <= 10) {
	    expected =  1;
	}
	//  - high speeds
	if (i > 48) {
	    expected =  20;
	}

	assert_int_equal(GetCWSpeedIndex(), expected);
    }
}

void test_GetSpeed(void **state) {
    SetCWSpeed(7);
    assert_int_equal(GetCWSpeed(), 12);
    SetCWSpeed(43);
    assert_int_equal(GetCWSpeed(), 44);
    SetCWSpeed(60);
    assert_int_equal(GetCWSpeed(), 50);
}


