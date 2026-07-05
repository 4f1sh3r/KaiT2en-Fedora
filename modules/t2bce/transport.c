#include "t2bce_transport.h"
#include "t2bce.h"

#include <linux/slab.h>

struct t2bce_client {
    struct t2bce_device *bce;
    struct device *dev;
    struct device_link *link;
};

struct t2bce_sq_ctx {
    t2bce_sq_completion completion;
    void *userdata;
};

static struct bce_queue_cq *to_bce_cq(struct t2bce_queue_cq *cq)
{
    return (struct bce_queue_cq *) cq;
}

static struct t2bce_queue_cq *to_t2bce_cq(struct bce_queue_cq *cq)
{
    return (struct t2bce_queue_cq *) cq;
}

static struct bce_queue_sq *to_bce_sq(struct t2bce_queue_sq *sq)
{
    return (struct bce_queue_sq *) sq;
}

static struct t2bce_queue_sq *to_t2bce_sq(struct bce_queue_sq *sq)
{
    return (struct t2bce_queue_sq *) sq;
}

static void t2bce_sq_completion_adapter(struct bce_queue_sq *sq)
{
    struct t2bce_sq_ctx *ctx = sq->userdata;

    ctx->completion(to_t2bce_sq(sq));
}

struct t2bce_client *t2bce_client_get(struct device *dev)
{
    struct t2bce_client *client;

    if (!global_bce)
        return NULL;

    client = kzalloc(sizeof(*client), GFP_KERNEL);
    if (!client)
        return NULL;

    client->bce = global_bce;
    client->dev = dev;
    client->link = device_link_add(dev, &global_bce->pci->dev,
            DL_FLAG_PM_RUNTIME | DL_FLAG_AUTOREMOVE_CONSUMER);
    return client;
}

void t2bce_client_put(struct t2bce_client *client)
{
    if (!client)
        return;

    if (client->link)
        device_link_del(client->link);
    kfree(client);
}

struct device *t2bce_client_dma_dev(struct t2bce_client *client)
{
    return &client->bce->pci->dev;
}

bool t2bce_client_no_state_resume(struct t2bce_client *client)
{
    return client->bce->vhci.no_state_resume;
}

void t2bce_client_set_audio(struct t2bce_client *client, struct aaudio_device *audio)
{
    client->bce->aaudio = audio;
}

void t2bce_client_clear_audio(struct t2bce_client *client, struct aaudio_device *audio)
{
    if (client && client->bce->aaudio == audio)
        client->bce->aaudio = NULL;
}

struct t2bce_queue_cq *t2bce_create_cq(struct t2bce_client *client, u32 el_count)
{
    return to_t2bce_cq(bce_create_cq(client->bce, el_count));
}

void t2bce_destroy_cq(struct t2bce_client *client, struct t2bce_queue_cq *cq)
{
    bce_destroy_cq(client->bce, to_bce_cq(cq));
}

struct t2bce_queue_sq *t2bce_create_sq(struct t2bce_client *client, struct t2bce_queue_cq *cq,
        const char *name, u32 el_count, enum dma_data_direction direction,
        t2bce_sq_completion compl, void *userdata)
{
    struct t2bce_sq_ctx *ctx;
    struct bce_queue_sq *sq;

    ctx = kzalloc(sizeof(*ctx), GFP_KERNEL);
    if (!ctx)
        return NULL;

    ctx->completion = compl;
    ctx->userdata = userdata;

    sq = bce_create_sq(client->bce, to_bce_cq(cq), name, el_count, direction,
            t2bce_sq_completion_adapter, ctx);
    if (!sq) {
        kfree(ctx);
        return NULL;
    }

    return to_t2bce_sq(sq);
}

void t2bce_destroy_sq(struct t2bce_client *client, struct t2bce_queue_sq *sq)
{
    struct bce_queue_sq *bce_sq = to_bce_sq(sq);
    struct t2bce_sq_ctx *ctx = bce_sq->userdata;

    bce_destroy_sq(client->bce, bce_sq);
    kfree(ctx);
}

void *t2bce_queue_sq_userdata(struct t2bce_queue_sq *sq)
{
    struct t2bce_sq_ctx *ctx = to_bce_sq(sq)->userdata;

    return ctx->userdata;
}

int t2bce_reserve_submission(struct t2bce_queue_sq *sq, unsigned long *timeout)
{
    return bce_reserve_submission(to_bce_sq(sq), timeout);
}

void t2bce_set_next_submission_single(struct t2bce_queue_sq *sq, dma_addr_t addr, size_t size)
{
    struct bce_qe_submission *submission = bce_next_submission(to_bce_sq(sq));

    bce_set_submission_single(submission, addr, size);
}

void t2bce_submit_to_device(struct t2bce_queue_sq *sq)
{
    bce_submit_to_device(to_bce_sq(sq));
}

void t2bce_notify_submission_complete(struct t2bce_queue_sq *sq)
{
    bce_notify_submission_complete(to_bce_sq(sq));
}

struct t2bce_sq_completion_data *t2bce_next_completion(struct t2bce_queue_sq *sq)
{
    return (struct t2bce_sq_completion_data *) bce_next_completion(to_bce_sq(sq));
}
