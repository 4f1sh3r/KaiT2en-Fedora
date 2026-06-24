/*
 * Copyright (C) 2012 Canonical
 *
 * This program is free software; you can redistribute it and/or
 * modify it under the terms of the GNU General Public License
 * as published by the Free Software Foundation; either version 2
 * of the License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program; if not, write to the Free Software
 * Foundation, Inc., 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301, USA.
 *
 */

#include <stdlib.h>
#include <stdio.h>
#include <errno.h>
#include <string.h>
#include <strings.h>
#include <ctype.h>

#define RBUF_SIZE	640

static unsigned int asc_to_int(char a)
{
	if (a >= 'A' && a <= 'F')
		return (a - 'A') + 10;
	else if (a >= 'a' && a <= 'f')
		return (a - 'a') + 10;
	else
		return a - '0';
}

static unsigned int hex_to_int(const char *h)
{
	return asc_to_int(*h) * 0x10 + asc_to_int(*(h + 1));
}

static unsigned int lhex_to_int(const char *h)
{
	return hex_to_int(h) * 0x100 + hex_to_int(h + 2);
}

static int check_sum(const char *str, int len)
{
	unsigned int sum, cal;
	int i;
	sum = hex_to_int(str + len - 2);
	for (cal = 0, i = 1; i < len - 2; i += 2)
		cal += hex_to_int(str + i);
	cal = (0x100 - cal) & 0xFF;
	return sum == cal;
}

static int check_hex_line(const char *str, int len)
{
	if ((str[0] != ':') || (len < 11) || !check_sum(str, len) ||
		((int)(hex_to_int(str + 1) * 2 + 11) != len))
		return 0;
	return 1;
}

static int is_hex_string(const char *str, int start, int len)
{
	int i;

	for (i = start; i < len; i++) {
		if (!isxdigit((unsigned char)str[i]))
			return 0;
	}

	return 1;
}

static int should_skip_line(const char *str, int len)
{
	if (len == 0)
		return 1;

	if (len == 1 && str[0] == '$')
		return 1;

	return 0;
}

static int replace_hex_suffix(const char *input, char *output, size_t output_len)
{
	size_t len;

	len = strlen(input);
	if (len + 5 > output_len)
		return -ENAMETOOLONG;

	strcpy(output, input);

	if (len >= 4 && strcasecmp(output + len - 4, ".hex") == 0) {
		strcpy(output + len - 4, ".hcd");
		return 0;
	}

	strcat(output, ".hcd");
	return 0;
}

static int write_vendor_command(FILE *ofp, unsigned char opcode, unsigned int addr)
{
	unsigned char obuf[7];

	obuf[0] = opcode;
	obuf[1] = 0xfc;
	obuf[2] = 4;
	obuf[3] = addr;
	obuf[4] = addr >> 8;
	obuf[5] = addr >> 16;
	obuf[6] = addr >> 24;

	return fwrite(obuf, sizeof(obuf), 1, ofp) == 1 ? 0 : -EIO;
}

int main(int argc, char *argv[])
{
	unsigned int addr = 0;
	unsigned int launch_addr;
	char output_path[4096];
	const char *ifn;
	const char *ofn;
	FILE *ifp, *ofp;
	char *rbuf;
	ssize_t len, i;
	size_t buflen = 0;
	unsigned int line = 0;

	if (argc != 2 && argc != 3) {
		printf("Usage: %s <input hex file> [output hcd file]\n", argv[0]);
		return 0;
	}

	ifn = argv[1];

	if (argc == 3) {
		ofn = argv[2];
	} else {
		if (replace_hex_suffix(ifn, output_path, sizeof(output_path)) != 0) {
			puts("output path is too long");
			return -ENAMETOOLONG;
		}
		ofn = output_path;
	}

	ifp = fopen(ifn, "r");
	ofp = fopen(ofn, "wb");
	if ((ifp == NULL) || (ofp == NULL)) {
		puts("failed to open file.");
		return -EIO;
	}

	rbuf = NULL;
	while ((len = getline(&rbuf, &buflen, ifp)) > 0) {
		int type;
		char obuf[7];
		unsigned int dest_addr;
		unsigned int offset;

		line++;

		while (len > 0 && ((rbuf[len - 1] == '\r') || (rbuf[len - 1] == '\n')))
			len--;

		if (should_skip_line(rbuf, len))
			continue;

		if (!is_hex_string(rbuf, 1, len)) {
			fprintf(stderr, "invalid character in hex file at line %u\n", line);
			return -EINVAL;
		}

		if (!check_hex_line(rbuf, len))
			goto format_err;

		type = hex_to_int(rbuf + 7);
		switch (type) {
			case 4:
				addr = lhex_to_int(rbuf + 9) * 0x10000;
				break;
			case 0:
				dest_addr = addr + lhex_to_int(rbuf + 3);
				obuf[0] = 0x4c;
				obuf[1] = 0xfc;
				obuf[2] = hex_to_int(rbuf + 1) + 4;
				obuf[3] = dest_addr;
				obuf[4] = dest_addr >> 8;
				obuf[5] = dest_addr >> 16;
				obuf[6] = dest_addr >> 24;
				if (fwrite(obuf, 7, 1, ofp) != 1)
					goto output_err;
				for (i = 0; i < hex_to_int(rbuf + 1); i++) {
					obuf[0] = hex_to_int(rbuf + 9 + i * 2);
					if (fwrite(obuf, 1, 1, ofp) != 1)
						goto output_err;
				}
				break;
			case 1:
				offset = lhex_to_int(rbuf + 3);
				launch_addr = addr + offset;
				if (write_vendor_command(ofp, 0x4e, launch_addr) != 0)
					goto output_err;
				goto end;
			default:
				return -EINVAL;
		}
	}

format_err:
	puts("hex file formatting error");
	return -EINVAL;

output_err:
	puts("error on writing output file");
	return -EIO;

end:
	printf("wrote %s\n", ofn);
	return 0;
}
