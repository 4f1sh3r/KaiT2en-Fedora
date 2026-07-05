#ifndef T2BCE_TRANSPORT_H
#define T2BCE_TRANSPORT_H

#include <linux/device.h>
#include <linux/dma-direction.h>

struct aaudio_device;
struct t2bce_client;
struct t2bce_queue_cq;
struct t2bce_queue_sq;

typedef void (*t2bce_sq_completion)(struct t2bce_queue_sq *sq);

struct t2bce_sq_completion_data {
    u32 status;
    u64 data_size;
    u64 result;
};

struct t2bce_client *t2bce_client_get(struct device *dev);
void t2bce_client_put(struct t2bce_client *client);

struct device *t2bce_client_dma_dev(struct t2bce_client *client);
bool t2bce_client_no_state_resume(struct t2bce_client *client);

void t2bce_client_set_audio(struct t2bce_client *client, struct aaudio_device *audio);
void t2bce_client_clear_audio(struct t2bce_client *client, struct aaudio_device *audio);

struct t2bce_queue_cq *t2bce_create_cq(struct t2bce_client *client, u32 el_count);
void t2bce_destroy_cq(struct t2bce_client *client, struct t2bce_queue_cq *cq);

struct t2bce_queue_sq *t2bce_create_sq(struct t2bce_client *client, struct t2bce_queue_cq *cq,
        const char *name, u32 el_count, enum dma_data_direction direction,
        t2bce_sq_completion compl, void *userdata);
void t2bce_destroy_sq(struct t2bce_client *client, struct t2bce_queue_sq *sq);

void *t2bce_queue_sq_userdata(struct t2bce_queue_sq *sq);

int t2bce_reserve_submission(struct t2bce_queue_sq *sq, unsigned long *timeout);
void t2bce_set_next_submission_single(struct t2bce_queue_sq *sq, dma_addr_t addr, size_t size);
void t2bce_submit_to_device(struct t2bce_queue_sq *sq);
void t2bce_notify_submission_complete(struct t2bce_queue_sq *sq);

struct t2bce_sq_completion_data *t2bce_next_completion(struct t2bce_queue_sq *sq);

#endif
