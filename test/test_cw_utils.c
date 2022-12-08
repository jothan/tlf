#include "test.h"

#include "../rustlf/rust.h"

// OBJECT ../src/cw_utils.o

void clear_line(int line) { abort(); }

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


